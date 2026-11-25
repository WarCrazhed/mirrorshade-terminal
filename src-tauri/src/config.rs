//! User configuration at `~/.config/mirrorshade/config.toml`.
//!
//! Missing file or keys fall back to defaults; the file is created with
//! defaults (and comments) on first run so the user can discover the options.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Lines kept per terminal pane (memory grows with this × panes).
    pub scrollback: u32,
    pub font_size: u16,
    pub font_family: String,
    pub ai: AiConfig,
    pub history: HistoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// "ollama" (local, default) or "claude" (cloud; only in builds with the `cloud-ai` feature).
    pub provider: String,
    pub ollama_url: String,
    pub ollama_model: String,
    pub lmstudio_url: String,
    /// Empty = whichever model LM Studio has loaded.
    pub lmstudio_model: String,
    pub claude_model: String,
    /// Max characters of terminal output sent as context (after redaction).
    pub max_context_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// Import ~/.zsh_history once, on first run.
    pub import_zsh_history: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scrollback: 5000,
            font_size: 13,
            font_family: r#""SF Mono", Menlo, Monaco, monospace"#.into(),
            ai: AiConfig::default(),
            history: HistoryConfig::default(),
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: "qwen2.5-coder:7b".into(),
            lmstudio_url: "http://127.0.0.1:1234".into(),
            lmstudio_model: String::new(),
            claude_model: "claude-opus-5-5".into(),
            max_context_chars: 6000,
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { import_zsh_history: true }
    }
}

const TEMPLATE_HEADER: &str = "\
# Mirrorshade Terminal — configuración
# Cambios se aplican al reiniciar la app (o con «Recargar configuración» en ⌘P).
#
# [ai]
# Proveedor y modelo se eligen dentro de la app (⌘P → «Elegir modelo de IA»).
#   ollama / lmstudio → IA local, nada sale de tu Mac
#   claude            → Claude API (solo si la app se compiló con --features cloud-ai)
# ollama_url y lmstudio_url solo aceptan direcciones de loopback (127.0.0.1 / localhost / ::1).

";

pub fn config_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".config/mirrorshade/config.toml")
}

pub fn load() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!("mirrorshade: config inválida ({e}); usando valores por defecto");
            Config::default()
        }),
        Err(_) => {
            let cfg = Config::default();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(body) = toml::to_string_pretty(&cfg) {
                let _ = std::fs::write(&path, format!("{TEMPLATE_HEADER}{body}"));
            }
            cfg
        }
    }
}
