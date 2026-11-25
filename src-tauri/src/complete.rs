//! Autocomplete engine: history (fuzzy + frecency), built-in command specs,
//! paths, git branches, package.json scripts and executables in PATH.
//!
//! Generators never execute programs: git branches are read straight from
//! `.git` (running `git` in an untrusted repo can execute code through config
//! such as `core.fsmonitor`), scripts from package.json.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatcherConfig, Matcher};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::history::{self, History};

const MAX_RESULTS: usize = 40;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggestion {
    pub label: String,
    /// Text that replaces the last `replace` characters of the input.
    pub insert: String,
    pub replace: usize,
    pub kind: &'static str,
    pub description: String,
}

// ---------- specs ----------

#[derive(Debug, Deserialize, Default)]
struct Spec {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    args: Option<ArgSpec>,
    #[serde(default)]
    options: Vec<OptSpec>,
    #[serde(default)]
    subcommands: Vec<Spec>,
}

#[derive(Debug, Deserialize, Default)]
struct OptSpec {
    name: Vec<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    takes_arg: bool,
}

#[derive(Debug, Deserialize, Default)]
struct ArgSpec {
    template: Option<String>,
    generator: Option<String>,
}

static SPECS: LazyLock<HashMap<String, Spec>> = LazyLock::new(|| {
    let list: Vec<Spec> =
        serde_json::from_str(include_str!("../specs/specs.json")).expect("valid specs.json");
    list.into_iter().map(|s| (s.name.clone(), s)).collect()
});

// ---------- tokenizing ----------

/// Splits a command line into tokens. Returns the tokens before the cursor
/// and the raw text of the last (partial) token, which may be empty.
fn tokenize(input: &str) -> (Vec<String>, String) {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut raw = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;
    for c in input.chars() {
        if escape {
            cur.push(c);
            raw.push(c);
            escape = false;
            continue;
        }
        match (c, quote) {
            ('\\', q) if q != Some('\'') => {
                escape = true;
                raw.push(c);
            }
            ('\'' | '"', None) => {
                quote = Some(c);
                raw.push(c);
            }
            (c2, Some(q)) if c2 == q => {
                quote = None;
                raw.push(c);
            }
            (c2, None) if c2.is_whitespace() => {
                if !raw.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                    raw.clear();
                }
            }
            // A new command after ; | && starts fresh.
            (';' | '|' | '&', None) => {
                tokens.clear();
                cur.clear();
                raw.clear();
            }
            _ => {
                cur.push(c);
                raw.push(c);
            }
        }
    }
    (tokens, raw)
}

fn unescape(raw: &str) -> String {
    let mut out = String::new();
    let mut esc = false;
    for c in raw.chars() {
        if esc {
            out.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c != '"' && c != '\'' {
            out.push(c);
        }
    }
    out
}

fn escape_path(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if " \t'\"\\$`!&;|()<>*?[]{}#~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// ---------- generators ----------

fn find_git_dir(cwd: &Path) -> Option<PathBuf> {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let g = d.join(".git");
        if g.is_dir() {
            return Some(g);
        }
        if g.is_file() {
            // worktree / submodule: "gitdir: <path>"
            let text = std::fs::read_to_string(&g).ok()?;
            let p = text.trim().strip_prefix("gitdir:")?.trim();
            let p = d.join(p);
            // A worktree's refs live in the common dir.
            let common = std::fs::read_to_string(p.join("commondir"))
                .ok()
                .map(|c| p.join(c.trim()))
                .unwrap_or(p);
            return Some(common);
        }
        dir = d.parent();
    }
    None
}

fn collect_refs(base: &Path, prefix: &str, out: &mut BTreeSet<String>, depth: usize) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(base) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let full = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if e.path().is_dir() {
            collect_refs(&e.path(), &full, out, depth + 1);
        } else if name != "HEAD" {
            out.insert(full);
        }
    }
}

pub fn git_branches(cwd: &Path) -> Vec<String> {
    let Some(git) = find_git_dir(cwd) else { return vec![] };
    let mut local = BTreeSet::new();
    let mut remote = BTreeSet::new();
    collect_refs(&git.join("refs/heads"), "", &mut local, 0);
    collect_refs(&git.join("refs/remotes"), "", &mut remote, 0);
    if let Ok(packed) = std::fs::read_to_string(git.join("packed-refs")) {
        for line in packed.lines() {
            let Some((_, r)) = line.split_once(' ') else { continue };
            if let Some(b) = r.strip_prefix("refs/heads/") {
                local.insert(b.to_string());
            } else if let Some(b) = r.strip_prefix("refs/remotes/") {
                if !b.ends_with("/HEAD") {
                    remote.insert(b.to_string());
                }
            }
        }
    }
    local.into_iter().chain(remote).collect()
}

fn npm_scripts(cwd: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(cwd.join("package.json")) else { return vec![] };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    v.get("scripts")
        .and_then(|s| s.as_object())
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn complete_paths(raw: &str, cwd: &Path, dirs_only: bool) -> Vec<Suggestion> {
    let typed = unescape(raw);
    let (dir_part, file_prefix) = match typed.rfind('/') {
        Some(i) => (&typed[..=i], &typed[i + 1..]),
        None => ("", typed.as_str()),
    };
    let base = if let Some(rest) = dir_part.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest)
    } else if dir_part == "~" {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
    } else if dir_part.starts_with('/') {
        PathBuf::from(dir_part)
    } else {
        cwd.join(dir_part)
    };
    let Ok(rd) = std::fs::read_dir(&base) else { return vec![] };
    let lower = file_prefix.to_lowercase();
    let mut items: Vec<(bool, String)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && !file_prefix.starts_with('.') {
                return None;
            }
            if !name.to_lowercase().starts_with(&lower) {
                return None;
            }
            // Follow symlinks for the dir check.
            let is_dir = std::fs::metadata(e.path()).map(|m| m.is_dir()).unwrap_or(false);
            if dirs_only && !is_dir {
                return None;
            }
            Some((is_dir, name))
        })
        .collect();
    items.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    // Keep what the user typed for the directory part (e.g. "~/"), escape the rest.
    let raw_dir = match raw.rfind('/') {
        Some(i) => &raw[..=i],
        None => "",
    };
    items
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(is_dir, name)| Suggestion {
            label: if is_dir { format!("{name}/") } else { name.clone() },
            insert: format!("{raw_dir}{}{}", escape_path(&name), if is_dir { "/" } else { " " }),
            replace: raw.chars().count(),
            kind: if is_dir { "folder" } else { "file" },
            description: String::new(),
        })
        .collect()
}

// ---------- engine ----------

#[derive(Default)]
pub struct Completer {
    executables: Mutex<HashMap<String, Vec<String>>>,
}

impl Completer {
    fn executables(&self, path_env: &str) -> Vec<String> {
        let mut cache = self.executables.lock().unwrap();
        if let Some(v) = cache.get(path_env) {
            return v.clone();
        }
        let mut set = BTreeSet::new();
        for dir in path_env.split(':').filter(|d| !d.is_empty()) {
            let Ok(rd) = std::fs::read_dir(dir) else { continue };
            for e in rd.flatten() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(m) = std::fs::metadata(e.path()) {
                        if m.is_file() && m.permissions().mode() & 0o111 != 0 {
                            set.insert(e.file_name().to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        let v: Vec<String> = set.into_iter().collect();
        cache.clear(); // only keep the latest PATH
        cache.insert(path_env.to_string(), v.clone());
        v
    }

    /// History lines fuzzy-matching the whole input, best first.
    fn history_matches(&self, input: &str, cwd: &str, hist: &History, limit: usize) -> Vec<Suggestion> {
        let needle = input.trim();
        if needle.chars().count() < 2 {
            return vec![];
        }
        let entries = hist.entries();
        let now = history::now();
        let mut matcher = Matcher::new(MatcherConfig::DEFAULT);
        let pattern = Pattern::parse(needle, CaseMatching::Smart, Normalization::Smart);
        let by_cmd: HashMap<&str, f64> = entries.iter().map(|e| (e.cmd.as_str(), e.score(now, cwd))).collect();
        let mut scored: Vec<(String, f64)> = pattern
            .match_list(entries.iter().map(|e| e.cmd.as_str()).filter(|c| !c.contains('\n') && *c != needle), &mut matcher)
            .into_iter()
            .map(|(cmd, fuzzy)| {
                let prefix_bonus = if cmd.starts_with(needle) { 2.0 } else { 1.0 };
                (cmd.to_string(), fuzzy as f64 * prefix_bonus * (1.0 + by_cmd.get(cmd).copied().unwrap_or(0.0)))
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored
            .into_iter()
            .take(limit)
            .map(|(cmd, _)| Suggestion {
                label: cmd.clone(),
                insert: cmd,
                replace: input.chars().count(),
                kind: "history",
                description: String::new(),
            })
            .collect()
    }

    /// `remote`: the line is being typed into ssh / docker exec / a nested
    /// shell. Local files, branches, scripts and executables don't apply
    /// there, so only history and command specs are used.
    pub fn complete(&self, input: &str, cwd: &str, path_env: &str, hist: &History, remote: bool) -> Vec<Suggestion> {
        let cwd_path = Path::new(cwd);
        if remote {
            return self.complete_remote(input, cwd, hist);
        }
        let (tokens, raw) = tokenize(input);
        let last = unescape(&raw);
        let replace = raw.chars().count();
        let mut out = self.history_matches(input, cwd, hist, 4);

        if tokens.is_empty() {
            // Completing the command name itself.
            if raw.contains('/') {
                out.extend(complete_paths(&raw, cwd_path, false));
                return out;
            }
            if last.is_empty() {
                return out;
            }
            let mut seen = BTreeSet::new();
            let mut names: Vec<Suggestion> = Vec::new();
            let mut spec_names: Vec<&Spec> = SPECS.values().filter(|s| s.name.starts_with(&last)).collect();
            spec_names.sort_by(|a, b| a.name.cmp(&b.name));
            for s in spec_names {
                seen.insert(s.name.clone());
                names.push(Suggestion {
                    label: s.name.clone(),
                    insert: format!("{} ", s.name),
                    replace,
                    kind: "command",
                    description: s.description.clone(),
                });
            }
            for exe in self.executables(path_env) {
                if names.len() >= MAX_RESULTS {
                    break;
                }
                if exe.starts_with(&last) && seen.insert(exe.clone()) {
                    names.push(Suggestion {
                        label: exe.clone(),
                        insert: format!("{exe} "),
                        replace,
                        kind: "command",
                        description: String::new(),
                    });
                }
            }
            out.extend(names);
            out.truncate(MAX_RESULTS);
            return out;
        }

        let Some(root) = SPECS.get(&tokens[0]) else {
            out.extend(complete_paths(&raw, cwd_path, false));
            out.truncate(MAX_RESULTS);
            return out;
        };

        // Walk subcommands through the completed tokens.
        let mut node = root;
        let mut option_arg_pending = false;
        for tok in &tokens[1..] {
            if option_arg_pending {
                option_arg_pending = false;
                continue;
            }
            if tok.starts_with('-') {
                option_arg_pending = !tok.contains('=')
                    && node.options.iter().any(|o| o.takes_arg && o.name.iter().any(|n| n == tok));
                continue;
            }
            if let Some(sub) = node.subcommands.iter().find(|s| &s.name == tok) {
                node = sub;
            }
        }

        if option_arg_pending {
            out.extend(complete_paths(&raw, cwd_path, false));
            out.truncate(MAX_RESULTS);
            return out;
        }

        if last.starts_with('-') {
            for o in &node.options {
                for n in o.name.iter().filter(|n| n.starts_with(&last)) {
                    out.push(Suggestion {
                        label: n.clone(),
                        insert: format!("{n} "),
                        replace,
                        kind: "option",
                        description: o.description.clone(),
                    });
                }
            }
            out.truncate(MAX_RESULTS);
            return out;
        }

        for s in node.subcommands.iter().filter(|s| s.name.starts_with(&last)) {
            out.push(Suggestion {
                label: s.name.clone(),
                insert: format!("{} ", s.name),
                replace,
                kind: "subcommand",
                description: s.description.clone(),
            });
        }

        match &node.args {
            Some(ArgSpec { generator: Some(g), .. }) => match g.as_str() {
                "git_branches" => {
                    for b in git_branches(cwd_path).into_iter().filter(|b| b.starts_with(&last)) {
                        out.push(Suggestion {
                            label: b.clone(),
                            insert: format!("{} ", escape_path(&b)),
                            replace,
                            kind: "branch",
                            description: String::new(),
                        });
                    }
                }
                "npm_scripts" => {
                    for (name, body) in npm_scripts(cwd_path).into_iter().filter(|(n, _)| n.starts_with(&last)) {
                        out.push(Suggestion {
                            label: name.clone(),
                            insert: format!("{} ", escape_path(&name)),
                            replace,
                            kind: "script",
                            description: body,
                        });
                    }
                }
                _ => {}
            },
            Some(ArgSpec { template: Some(t), .. }) => {
                out.extend(complete_paths(&raw, cwd_path, t == "folders"));
            }
            _ if node.subcommands.is_empty() => out.extend(complete_paths(&raw, cwd_path, false)),
            _ => {}
        }
        out.truncate(MAX_RESULTS);
        out
    }
}

impl Completer {
    fn complete_remote(&self, input: &str, cwd: &str, hist: &History) -> Vec<Suggestion> {
        let (tokens, raw) = tokenize(input);
        let last = unescape(&raw);
        let replace = raw.chars().count();
        let mut out = self.history_matches(input, cwd, hist, 6);
        if tokens.is_empty() {
            let mut names: Vec<&Spec> = SPECS.values().filter(|s| !last.is_empty() && s.name.starts_with(&last)).collect();
            names.sort_by(|a, b| a.name.cmp(&b.name));
            out.extend(names.into_iter().map(|s| Suggestion {
                label: s.name.clone(),
                insert: format!("{} ", s.name),
                replace,
                kind: "command",
                description: s.description.clone(),
            }));
            out.truncate(MAX_RESULTS);
            return out;
        }
        let Some(root) = SPECS.get(&tokens[0]) else { return out };
        let mut node = root;
        for tok in &tokens[1..] {
            if let Some(sub) = node.subcommands.iter().find(|s| &s.name == tok) {
                node = sub;
            }
        }
        if last.starts_with('-') {
            for o in &node.options {
                for n in o.name.iter().filter(|n| n.starts_with(&last)) {
                    out.push(Suggestion { label: n.clone(), insert: format!("{n} "), replace, kind: "option", description: o.description.clone() });
                }
            }
        } else {
            for s in node.subcommands.iter().filter(|s| s.name.starts_with(&last)) {
                out.push(Suggestion {
                    label: s.name.clone(),
                    insert: format!("{} ", s.name),
                    replace,
                    kind: "subcommand",
                    description: s.description.clone(),
                });
            }
        }
        out.truncate(MAX_RESULTS);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_mode_skips_local_filesystem() {
        let dir = std::env::temp_dir().join(format!("ms-r-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("localonly")).unwrap();
        let cwd = dir.to_str().unwrap();
        let c = Completer::default();
        let h = History::in_memory();
        h.add("php artisan migrate", "remote:docker app", 0, history::now());
        assert!(c.complete("cd loc", cwd, "", &h, false).iter().any(|s| s.label == "localonly/"));
        assert!(c.complete("cd loc", cwd, "", &h, true).is_empty());
        let r = c.complete("git chec", cwd, "", &h, true);
        assert_eq!(labels(&r), vec!["checkout"]);
        let r = c.complete("php art", "remote:docker app", "", &h, true);
        assert_eq!(r[0].label, "php artisan migrate");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn labels(v: &[Suggestion]) -> Vec<&str> {
        v.iter().map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn tokenizes_quotes_and_escapes() {
        assert_eq!(tokenize("git commit -m \"a b\" "), (vec!["git".into(), "commit".into(), "-m".into(), "a b".into()], String::new()));
        assert_eq!(tokenize("cd My\\ Fo"), (vec!["cd".into()], "My\\ Fo".into()));
        assert_eq!(tokenize("ls | gr"), (vec![], "gr".into()));
    }

    #[test]
    fn completes_subcommands_and_options() {
        let c = Completer::default();
        let h = History::in_memory();
        let r = c.complete("git chec", "/", "", &h, false);
        assert_eq!(labels(&r), vec!["checkout"]);
        assert_eq!(r[0].insert, "checkout ");
        assert_eq!(r[0].replace, 4);
        let r = c.complete("docker run --r", "/", "", &h, false);
        assert_eq!(labels(&r), vec!["--rm"]);
        let r = c.complete("php artisan migrate:f", "/", "", &h, false);
        assert_eq!(labels(&r), vec!["migrate:fresh"]);
    }

    #[test]
    fn completes_paths_and_scripts() {
        let dir = std::env::temp_dir().join(format!("ms-c-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("My Folder")).unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        std::fs::write(dir.join("package.json"), r#"{"scripts":{"dev":"vite","deploy":"x"}}"#).unwrap();
        let cwd = dir.to_str().unwrap();
        let c = Completer::default();
        let h = History::in_memory();

        let r = c.complete("cd My", cwd, "", &h, false);
        assert_eq!(r[0].insert, "My\\ Folder/");
        let r = c.complete("cat no", cwd, "", &h, false);
        assert_eq!(r[0].insert, "notes.txt ");
        let mut r = labels(&c.complete("pnpm run de", cwd, "", &h, false)).into_iter().map(String::from).collect::<Vec<_>>();
        r.sort();
        assert_eq!(r, vec!["deploy", "dev"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reads_git_branches_without_running_git() {
        let dir = std::env::temp_dir().join(format!("ms-g-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git/refs/heads/feature")).unwrap();
        std::fs::create_dir_all(dir.join(".git/refs/remotes/origin")).unwrap();
        std::fs::write(dir.join(".git/refs/heads/main"), "x").unwrap();
        std::fs::write(dir.join(".git/refs/heads/feature/login"), "x").unwrap();
        std::fs::write(dir.join(".git/refs/remotes/origin/HEAD"), "x").unwrap();
        std::fs::write(dir.join(".git/packed-refs"), "abc refs/heads/old\nabc refs/remotes/origin/main\n").unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let b = git_branches(&dir.join("sub"));
        assert_eq!(b, vec!["feature/login", "main", "old", "origin/main"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn history_fuzzy_on_top() {
        let c = Completer::default();
        let h = History::in_memory();
        h.add("docker compose up -d --build", "/", 0, history::now());
        let r = c.complete("dcup", "/", "", &h, false);
        assert_eq!(r.first().map(|s| s.kind), Some("history"));
        assert_eq!(r[0].replace, 4);
    }
}
