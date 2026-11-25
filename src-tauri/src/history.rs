//! Local command history (SQLite, file mode 0600) with frecency ranking.
//!
//! Privacy rules on insert: commands starting with a space are not stored
//! (zsh HIST_IGNORE_SPACE convention), and neither is anything the secret
//! redactor flags.

use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Entry {
    pub cmd: String,
    pub count: i64,
    pub last_used: i64,
    pub last_cwd: String,
    pub last_exit: i64,
}

impl Entry {
    /// Frecency: frequency weighted by how recently it was used; failed
    /// commands rank lower.
    pub fn score(&self, now: i64, cwd: &str) -> f64 {
        let age = now - self.last_used;
        let recency = match age {
            a if a < 3600 => 4.0,
            a if a < 86_400 => 2.0,
            a if a < 604_800 => 1.0,
            _ => 0.5,
        };
        let mut s = (self.count as f64).sqrt() * recency;
        if !cwd.is_empty() && self.last_cwd == cwd {
            s *= 1.5;
        }
        if self.last_exit != 0 {
            s *= 0.4;
        }
        s
    }
}

pub struct History {
    conn: Mutex<Connection>,
    /// In-memory snapshot for fuzzy matching; rebuilt after each insert.
    cache: Mutex<Option<Vec<Entry>>>,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS history (
                cmd TEXT PRIMARY KEY,
                count INTEGER NOT NULL DEFAULT 1,
                last_used INTEGER NOT NULL,
                last_cwd TEXT NOT NULL DEFAULT '',
                last_exit INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT);",
        )?;
        Ok(Self { conn: Mutex::new(conn), cache: Mutex::new(None) })
    }

    pub fn in_memory() -> Self {
        Self::open(Path::new(":memory:")).expect("in-memory sqlite")
    }

    pub fn should_store(cmd: &str) -> bool {
        let t = cmd.trim();
        !t.is_empty() && !cmd.starts_with(' ') && t.len() < 4096 && !crate::redact::contains_secret(t)
    }

    pub fn add(&self, cmd: &str, cwd: &str, exit: i64, at: i64) {
        if !Self::should_store(cmd) {
            return;
        }
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO history (cmd, count, last_used, last_cwd, last_exit) VALUES (?1, 1, ?2, ?3, ?4)
             ON CONFLICT(cmd) DO UPDATE SET count = count + 1, last_used = ?2, last_cwd = ?3, last_exit = ?4",
            params![cmd.trim(), at, cwd, exit],
        );
        *self.cache.lock().unwrap() = None;
    }

    /// One-time import of ~/.zsh_history (both plain and EXTENDED_HISTORY format).
    pub fn import_zsh_history(&self, path: &Path) {
        {
            let conn = self.conn.lock().unwrap();
            let done: Option<String> = conn
                .query_row("SELECT value FROM meta WHERE key = 'zsh_imported'", [], |r| r.get(0))
                .ok();
            if done.is_some() {
                return;
            }
        }
        let Ok(bytes) = std::fs::read(path) else { return };
        let text = String::from_utf8_lossy(&bytes);
        let mut entries: Vec<(String, i64)> = Vec::new();
        let mut pending = String::new();
        let mut pending_ts = 0i64;
        for line in text.lines() {
            if pending.is_empty() {
                // ": 1700000000:0;command"
                if let Some(rest) = line.strip_prefix(": ") {
                    if let Some((meta, cmd)) = rest.split_once(';') {
                        pending_ts = meta.split(':').next().and_then(|t| t.parse().ok()).unwrap_or(0);
                        pending = cmd.to_string();
                    } else {
                        continue;
                    }
                } else {
                    pending_ts = 0;
                    pending = line.to_string();
                }
            } else {
                pending.push('\n');
                pending.push_str(line);
            }
            // Multi-line commands end lines with a backslash continuation.
            if pending.ends_with('\\') {
                pending.pop();
                continue;
            }
            entries.push((std::mem::take(&mut pending), pending_ts));
        }
        let fallback_ts = now() - 30 * 86_400;
        let mut conn = self.conn.lock().unwrap();
        if let Ok(tx) = conn.transaction() {
            for (cmd, ts) in entries {
                if !Self::should_store(&cmd) {
                    continue;
                }
                let ts = if ts > 0 { ts } else { fallback_ts };
                let _ = tx.execute(
                    "INSERT INTO history (cmd, count, last_used) VALUES (?1, 1, ?2)
                     ON CONFLICT(cmd) DO UPDATE SET count = count + 1, last_used = MAX(last_used, ?2)",
                    params![cmd.trim(), ts],
                );
            }
            let _ = tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('zsh_imported', '1')", []);
            let _ = tx.commit();
        }
        *self.cache.lock().unwrap() = None;
    }

    /// All entries (cached). Fine for tens of thousands of rows.
    pub fn entries(&self) -> Vec<Entry> {
        let mut cache = self.cache.lock().unwrap();
        if cache.is_none() {
            let conn = self.conn.lock().unwrap();
            let rows = conn
                .prepare("SELECT cmd, count, last_used, last_cwd, last_exit FROM history")
                .and_then(|mut st| {
                    st.query_map([], |r| {
                        Ok(Entry {
                            cmd: r.get(0)?,
                            count: r.get(1)?,
                            last_used: r.get(2)?,
                            last_cwd: r.get(3)?,
                            last_exit: r.get(4)?,
                        })
                    })
                    .map(|it| it.filter_map(Result::ok).collect::<Vec<_>>())
                })
                .unwrap_or_default();
            *cache = Some(rows);
        }
        cache.clone().unwrap_or_default()
    }

    /// Best history entry that extends `prefix` (fish-style autosuggestion).
    pub fn ghost(&self, prefix: &str, cwd: &str) -> Option<String> {
        if prefix.trim().is_empty() {
            return None;
        }
        let t = now();
        self.entries()
            .into_iter()
            .filter(|e| e.cmd.len() > prefix.len() && e.cmd.starts_with(prefix) && !e.cmd.contains('\n'))
            .max_by(|a, b| a.score(t, cwd).total_cmp(&b.score(t, cwd)))
            .map(|e| e.cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_prefers_frequent_and_recent() {
        let h = History::in_memory();
        let t = now();
        h.add("git status", "/p", 0, t - 10);
        h.add("git status", "/p", 0, t - 5);
        h.add("git stash pop", "/p", 0, t - 400_000);
        assert_eq!(h.ghost("git st", "/p").as_deref(), Some("git status"));
        assert_eq!(h.ghost("git stas", "/p").as_deref(), Some("git stash pop"));
        assert_eq!(h.ghost("git status", "/p"), None); // nothing longer
    }

    #[test]
    fn skips_private_commands() {
        let h = History::in_memory();
        h.add(" secret-cmd", "", 0, now());
        h.add("export GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789", "", 0, now());
        assert!(h.entries().is_empty());
    }

    #[test]
    fn imports_extended_zsh_history() {
        let dir = std::env::temp_dir().join(format!("ms-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("hist");
        std::fs::write(&f, ": 1700000000:0;ls -la\n: 1700000001:0;echo a \\\nb\nplain cmd\n").unwrap();
        let h = History::in_memory();
        h.import_zsh_history(&f);
        h.import_zsh_history(&f); // idempotent
        let mut cmds: Vec<_> = h.entries().into_iter().map(|e| (e.cmd, e.count)).collect();
        cmds.sort();
        assert_eq!(
            cmds,
            vec![("echo a \nb".into(), 1), ("ls -la".into(), 1), ("plain cmd".into(), 1)]
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
