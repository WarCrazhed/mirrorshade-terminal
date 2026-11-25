//! PTY session management.
//!
//! Each terminal tab owns one PTY running the user's login shell. Output is
//! streamed to the WebView through a Tauri `Channel` as raw bytes (no JSON
//! number arrays), so xterm.js receives an `ArrayBuffer` and handles UTF-8
//! sequences split across reads on its own.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;

struct Session {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct PtyState {
    sessions: Mutex<HashMap<u32, Session>>,
    next_id: AtomicU32,
    /// Root of our shell integration files (None = integration unavailable).
    pub integration_dir: Mutex<Option<std::path::PathBuf>>,
}

#[derive(serde::Serialize)]
pub struct Spawned {
    id: u32,
    /// Secret the shell hooks include in custom OSC sequences; the frontend
    /// ignores sequences without it, so program output can't spoof them.
    nonce: String,
    integration: bool,
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn user_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into())
}

fn first_existing(paths: &[&str]) -> Option<String> {
    paths.iter().find(|p| std::path::Path::new(p).is_file()).map(|p| p.to_string())
}

/// Resolve the shell to run: "auto" = $SHELL, or a specific zsh/bash/fish.
/// Prefers Homebrew builds (macOS ships bash 3.2; fish isn't preinstalled).
pub fn resolve_shell(choice: &str) -> String {
    match choice {
        "zsh" => first_existing(&["/bin/zsh", "/opt/homebrew/bin/zsh", "/usr/local/bin/zsh"]),
        "bash" => first_existing(&["/opt/homebrew/bin/bash", "/usr/local/bin/bash", "/bin/bash"]),
        "fish" => first_existing(&["/opt/homebrew/bin/fish", "/usr/local/bin/fish"]),
        _ => None,
    }
    .unwrap_or_else(user_shell)
}

/// Which installed shells the settings screen can offer.
#[tauri::command]
pub fn shells_available() -> Vec<String> {
    ["zsh", "bash", "fish"]
        .into_iter()
        .filter(|s| {
            let r = resolve_shell(s);
            crate::shell_integration::shell_kind(&r) == Some(*s)
        })
        .map(String::from)
        .collect()
}

/// Spawn an interactive shell in a new PTY. Returns the session id.
#[tauri::command]
pub fn pty_spawn(
    state: State<'_, PtyState>,
    cols: u16,
    rows: u16,
    on_data: Channel<InvokeResponseBody>,
    on_exit: Channel<i32>,
    prompt_theme: Option<String>,
    shell: Option<String>,
    cwd: Option<String>,
) -> Result<Spawned, String> {
    let pair = native_pty_system()
        .openpty(size(cols, rows))
        .map_err(|e| e.to_string())?;

    let shell = resolve_shell(shell.as_deref().unwrap_or("auto"));
    let kind = crate::shell_integration::shell_kind(&shell);
    let mut cmd = CommandBuilder::new(&shell);
    // New panes open in the same folder as the pane they were split from.
    let start_dir = cwd
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_dir())
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from));
    if let Some(dir) = start_dir {
        cmd.cwd(dir);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "Mirrorshade");
    cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    if std::env::var_os("LANG").is_none() {
        cmd.env("LANG", "en_US.UTF-8");
    }
    let nonce = crate::shell_integration::nonce();
    let theme = prompt_theme
        .filter(|t| crate::prefs::PROMPT_THEMES.contains(&t.as_str()) && t != "none")
        .unwrap_or_default();
    let root = state.integration_dir.lock().unwrap().clone();
    let integration = match (root, kind) {
        (Some(root), Some(kind)) => {
            cmd.env("MIRRORSHADE_NONCE", &nonce);
            cmd.env("MIRRORSHADE_PROMPT_FILE", root.join("prompt-state"));
            if !theme.is_empty() {
                cmd.env("MIRRORSHADE_PROMPT_THEME", &theme);
            }
            match kind {
                "zsh" => {
                    let dir = root.join("zsh");
                    // login shell: apps launched from Finder get a minimal PATH otherwise
                    cmd.arg("-l");
                    if let Some(user_zd) = std::env::var_os("ZDOTDIR") {
                        cmd.env("MIRRORSHADE_USER_ZDOTDIR", user_zd);
                    }
                    cmd.env("ZDOTDIR", &dir);
                    cmd.env("MIRRORSHADE_INTEGRATION_DIR", &dir);
                }
                "bash" => {
                    // --rcfile only applies to non-login shells; our rcfile
                    // emulates login startup (profile files) itself.
                    cmd.arg("--rcfile");
                    cmd.arg(root.join("bash").join("mirrorshade.bash"));
                    cmd.arg("-i");
                }
                _ => {
                    // fish: runs after the user's config.fish
                    cmd.arg("-l");
                    cmd.arg("--init-command");
                    cmd.arg(format!("source '{}'", root.join("fish").join("mirrorshade.fish").display()));
                }
            }
            true
        }
        _ => {
            cmd.arg("-l");
            false
        }
    };

    let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    // Parent no longer needs the slave end; dropping it lets reads hit EOF on exit.
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let killer = child.clone_killer();

    let id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    state.sessions.lock().unwrap().insert(
        id,
        Session {
            writer,
            master: pair.master,
            killer,
        },
    );

    thread::Builder::new()
        .name(format!("pty-{id}"))
        .spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if on_data
                            .send(InvokeResponseBody::Raw(buf[..n].to_vec()))
                            .is_err()
                        {
                            break; // webview gone
                        }
                    }
                }
            }
            let code = child
                .wait()
                .map(|s| s.exit_code() as i32)
                .unwrap_or(-1);
            let _ = on_exit.send(code);
        })
        .map_err(|e| e.to_string())?;

    Ok(Spawned { id, nonce, integration })
}

#[tauri::command]
pub fn pty_write(state: State<'_, PtyState>, id: u32, data: String) -> Result<(), String> {
    let mut sessions = state.sessions.lock().unwrap();
    let s = sessions.get_mut(&id).ok_or("unknown session")?;
    s.writer
        .write_all(data.as_bytes())
        .and_then(|_| s.writer.flush())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_resize(state: State<'_, PtyState>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = state.sessions.lock().unwrap();
    let s = sessions.get(&id).ok_or("unknown session")?;
    s.master.resize(size(cols, rows)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_kill(state: State<'_, PtyState>, id: u32) -> Result<(), String> {
    if let Some(mut s) = state.sessions.lock().unwrap().remove(&id) {
        let _ = s.killer.kill();
    }
    Ok(())
}

/// Kill every shell when the app exits so no orphaned processes remain.
pub fn kill_all(state: &PtyState) {
    for (_, mut s) in state.sessions.lock().unwrap().drain() {
        let _ = s.killer.kill();
    }
}
