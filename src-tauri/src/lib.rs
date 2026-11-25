mod ai;
mod complete;
mod config;
mod danger;
mod history;
mod links;
mod prefs;
mod pty;
mod redact;
mod shell_integration;
mod themes;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex, RwLock};

use tauri::{Manager, RunEvent, State, WebviewWindow};

pub struct AppState {
    pub config: RwLock<config::Config>,
    pub prefs: RwLock<prefs::Prefs>,
    pub data_dir: PathBuf,
    pub history: Arc<history::History>,
    pub completer: complete::Completer,
    pub ai_tasks: Arc<Mutex<HashMap<u32, tauri::async_runtime::JoinHandle<()>>>>,
    pub next_ai: AtomicU32,
}

#[derive(serde::Serialize)]
struct FrontendConfig {
    scrollback: u32,
    font_size: u16,
    font_family: String,
    config_path: String,
    prefs: prefs::Prefs,
}

fn frontend_config(state: &AppState) -> FrontendConfig {
    let c = state.config.read().unwrap();
    let p = state.prefs.read().unwrap().clone();
    FrontendConfig {
        scrollback: c.scrollback,
        font_size: c.font_size, // base size; prefs.font_size overrides it when non-zero
        font_family: c.font_family.clone(),
        config_path: config::config_path().to_string_lossy().to_string(),
        prefs: p,
    }
}

#[tauri::command]
fn get_config(state: State<'_, AppState>) -> FrontendConfig {
    frontend_config(&state)
}

#[tauri::command]
fn reload_config(state: State<'_, AppState>) -> FrontendConfig {
    *state.config.write().unwrap() = config::load();
    frontend_config(&state)
}

/// Save UI preferences (validated) and apply the native blur.
#[tauri::command]
fn prefs_set(window: WebviewWindow, state: State<'_, AppState>, prefs: prefs::Prefs) -> prefs::Prefs {
    let clean = prefs.sanitized(cfg!(feature = "cloud-ai"));
    let blur_changed = state.prefs.read().unwrap().blur_radius != clean.blur_radius;
    *state.prefs.write().unwrap() = clean.clone();
    prefs::save(&state.data_dir, &clean);
    if blur_changed {
        apply_blur(&window, clean.blur_radius);
    }
    clean
}

/// Blur whatever is behind the (transparent) window with the given radius.
/// Uses the same private CoreGraphics call as kitty / iTerm2, which — unlike
/// NSVisualEffectView materials — lets us choose the intensity.
fn apply_blur(window: &WebviewWindow, radius: u8) {
    #[cfg(target_os = "macos")]
    {
        let w = window.clone();
        let _ = window.run_on_main_thread(move || {
            let Ok(ns_window) = w.ns_window() else { return };
            if let Err(e) = macos_blur::set_radius(ns_window, radius) {
                eprintln!("mirrorshade: no se pudo cambiar el desenfoque ({e})");
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (window, radius);
}

#[cfg(target_os = "macos")]
mod macos_blur {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSDefaultConnectionForThread() -> i32;
        fn CGSSetWindowBackgroundBlurRadius(cid: i32, wid: i32, radius: i32) -> i32;
    }

    pub fn set_radius(ns_window: *mut c_void, radius: u8) -> Result<(), String> {
        if ns_window.is_null() {
            return Err("sin NSWindow".into());
        }
        // SAFETY: Tauri hands us a valid NSWindow pointer and we're on the main thread.
        let number: isize = unsafe { msg_send![&*(ns_window as *mut AnyObject), windowNumber] };
        let err = unsafe {
            CGSSetWindowBackgroundBlurRadius(CGSDefaultConnectionForThread(), number as i32, radius as i32)
        };
        if err == 0 { Ok(()) } else { Err(format!("CGError {err}")) }
    }
}

/// Tell running shells about the prompt style, gradient and theme colors.
/// They re-read this file before every prompt, so changes apply live.
#[tauri::command]
fn prompt_sync(state: State<'_, AppState>, colors: Vec<String>) -> Result<(), String> {
    let p = state.prefs.read().unwrap().clone();
    let body = prefs::prompt_state(&p, &colors).ok_or("colores inválidos")?;
    let file = state.data_dir.join("shell-integration").join("prompt-state");
    std::fs::write(&file, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[tauri::command]
fn themes_list(state: State<'_, AppState>) -> Vec<themes::CustomTheme> {
    themes::load(&state.data_dir)
}

#[tauri::command]
fn theme_save(state: State<'_, AppState>, theme: themes::CustomTheme) -> Result<Vec<themes::CustomTheme>, String> {
    themes::save(&state.data_dir, theme)
}

#[tauri::command]
fn theme_delete(state: State<'_, AppState>, id: String) -> Result<Vec<themes::CustomTheme>, String> {
    themes::delete(&state.data_dir, &id)
}

#[tauri::command]
fn history_add(state: State<'_, AppState>, cmd: String, cwd: String, exit_code: i64) {
    state.history.add(&cmd, &cwd, exit_code, history::now());
}

#[tauri::command]
fn complete_ghost(state: State<'_, AppState>, input: String, cwd: String) -> Option<String> {
    state.history.ghost(&input, &cwd)
}

#[tauri::command]
fn complete_list(
    state: State<'_, AppState>,
    input: String,
    cwd: String,
    path: String,
    remote: Option<bool>,
) -> Vec<complete::Suggestion> {
    state.completer.complete(&input, &cwd, &path, &state.history, remote.unwrap_or(false))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(pty::PtyState::default())
        .setup(|app| {
            let cfg = config::load();
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let hist = Arc::new(
                history::History::open(&data_dir.join("history.db")).unwrap_or_else(|e| {
                    eprintln!("mirrorshade: no se pudo abrir el historial ({e}); usando memoria");
                    history::History::in_memory()
                }),
            );
            if cfg.history.import_zsh_history {
                let h = hist.clone();
                std::thread::spawn(move || {
                    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                    h.import_zsh_history(&home.join(".zsh_history"));
                });
            }

            match shell_integration::install(&data_dir) {
                Ok(dir) => *app.state::<pty::PtyState>().integration_dir.lock().unwrap() = Some(dir),
                Err(e) => eprintln!("mirrorshade: sin integración de shell ({e})"),
            }

            let cloud = cfg!(feature = "cloud-ai");
            let first_run = !prefs::path(&data_dir).exists();
            let mut p = prefs::load(&data_dir, cloud);
            if first_run {
                p.ai_provider = cfg.ai.provider.clone();
                p = p.sanitized(cloud);
            }
            if p.blur_radius > 0 {
                if let Some(w) = app.get_webview_window("main") {
                    apply_blur(&w, p.blur_radius);
                }
            }

            app.manage(AppState {
                config: RwLock::new(cfg),
                prefs: RwLock::new(p),
                data_dir,
                history: hist,
                completer: complete::Completer::default(),
                ai_tasks: Arc::new(Mutex::new(HashMap::new())),
                next_ai: AtomicU32::new(0),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::shells_available,
            links::open_url,
            links::open_path,
            links::paths_exist,
            get_config,
            reload_config,
            prefs_set,
            prompt_sync,
            themes_list,
            theme_save,
            theme_delete,
            history_add,
            complete_ghost,
            complete_list,
            ai::ai_status,
            ai::ai_list_models,
            ai::ai_select_model,
            ai::ai_set_api_key,
            ai::ai_ask,
            ai::ai_cancel,
            ai::check_command,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Mirrorshade")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                pty::kill_all(&app.state::<pty::PtyState>());
            }
        });
}
