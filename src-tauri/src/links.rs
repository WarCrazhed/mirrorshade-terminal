//! ⌘+click on URLs and file paths printed in the terminal.
//!
//! Terminal output is untrusted (a program can print any path), so opening
//! must never run anything: executables, app bundles and scripts are only
//! *revealed* in Finder; regular files open as text in an editor; URLs are
//! limited to http(s). Programs are started directly (no shell).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Extensions macOS would execute or install when "opened".
const RISKY_EXT: &[&str] = &[
    "app", "command", "tool", "terminal", "sh", "bash", "zsh", "fish", "csh", "ksh", "py", "rb", "pl", "php",
    "js", "mjs", "jar", "scpt", "scptd", "applescript", "workflow", "action", "pkg", "mpkg", "dmg", "prefpane",
    "kext", "plugin", "bundle", "framework", "webloc", "inetloc", "fileloc", "url", "desktop", "osax", "saver",
    "mobileconfig", "shortcut",
];

fn spawn(program: &str, args: &[&str]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn resolve(path: &str, cwd: &str) -> Option<PathBuf> {
    if path.is_empty() || path.len() > 1024 || path.contains('\0') {
        return None;
    }
    let p = if let Some(rest) = path.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME")?).join(rest)
    } else if path == "~" {
        PathBuf::from(std::env::var_os("HOME")?)
    } else if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        if cwd.is_empty() || !cwd.starts_with('/') {
            return None;
        }
        Path::new(cwd).join(path)
    };
    p.canonicalize().ok()
}

fn is_risky(p: &Path) -> bool {
    let ext_risky = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| RISKY_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false);
    #[cfg(unix)]
    let exec = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    };
    #[cfg(not(unix))]
    let exec = false;
    ext_risky || exec
}

fn editor_cli() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let candidates = [
        PathBuf::from("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"),
        home.join("Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"),
        PathBuf::from("/Applications/Cursor.app/Contents/Resources/app/bin/cursor"),
        home.join("Applications/Cursor.app/Contents/Resources/app/bin/cursor"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// What ⌘+click on a path will do (for the hover tooltip and tests).
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PathAction {
    Folder,
    Reveal,
    Edit,
}

pub fn action_for(p: &Path) -> PathAction {
    if is_risky(p) {
        PathAction::Reveal // also catches .app bundles, which are directories
    } else if p.is_dir() {
        PathAction::Folder
    } else {
        PathAction::Edit
    }
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let u = reqwest::Url::parse(url.trim()).map_err(|_| "URL inválida".to_string())?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("Solo se abren enlaces http(s)".into());
    }
    spawn("/usr/bin/open", &[u.as_str()])
}

#[tauri::command]
pub fn open_path(path: String, cwd: String, line: Option<u32>, col: Option<u32>) -> Result<String, String> {
    let p = resolve(&path, &cwd).ok_or("La ruta no existe")?;
    let s = p.to_string_lossy().to_string();
    match action_for(&p) {
        PathAction::Reveal => spawn("/usr/bin/open", &["-R", &s]).map(|_| "Mostrado en Finder".into()),
        PathAction::Folder => spawn("/usr/bin/open", &[&s]).map(|_| "Abierto en Finder".into()),
        PathAction::Edit => {
            if let Some(cli) = editor_cli() {
                let target = match (line, col) {
                    (Some(l), Some(c)) => format!("{s}:{l}:{c}"),
                    (Some(l), None) => format!("{s}:{l}"),
                    _ => s.clone(),
                };
                spawn(&cli.to_string_lossy(), &["-g", &target]).map(|_| "Abierto en el editor".into())
            } else {
                // -t: always a text editor, never the file's default handler.
                spawn("/usr/bin/open", &["-t", &s]).map(|_| "Abierto en el editor de texto".into())
            }
        }
    }
}

/// Which of the candidate paths (seen in terminal output) exist. Used to
/// decide what to underline; capped to keep hover cheap.
#[tauri::command]
pub fn paths_exist(paths: Vec<String>, cwd: String) -> Vec<bool> {
    paths.iter().take(64).map(|p| resolve(p, &cwd).is_some()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_and_classifies() {
        let dir = std::env::temp_dir().join(format!("ms-links-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("Evil.app/Contents")).unwrap();
        std::fs::write(dir.join("src/main.ts"), "x").unwrap();
        std::fs::write(dir.join("run.command"), "x").unwrap();
        std::fs::write(dir.join("script"), "#!/bin/sh").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join("script"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let cwd = dir.to_str().unwrap();
        assert_eq!(action_for(&resolve("src/main.ts", cwd).unwrap()), PathAction::Edit);
        assert_eq!(action_for(&resolve("src", cwd).unwrap()), PathAction::Folder);
        assert_eq!(action_for(&resolve("Evil.app", cwd).unwrap()), PathAction::Reveal);
        assert_eq!(action_for(&resolve("run.command", cwd).unwrap()), PathAction::Reveal);
        assert_eq!(action_for(&resolve("script", cwd).unwrap()), PathAction::Reveal);
        assert!(resolve("nope.txt", cwd).is_none());
        assert!(resolve("relative", "").is_none());
        assert_eq!(paths_exist(vec!["src/main.ts".into(), "x/y".into()], cwd.into()), vec![true, false]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_http_urls() {
        assert!(open_url("file:///etc/passwd".into()).is_err());
        assert!(open_url("javascript:alert(1)".into()).is_err());
        assert!(open_url("ssh://x".into()).is_err());
    }
}
