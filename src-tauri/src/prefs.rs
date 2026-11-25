//! UI/AI preferences changed from inside the app (theme, opacity, blur,
//! prompt theme, selected model). Stored as JSON in the app data dir so we
//! never rewrite the user's commented config.toml.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PROVIDERS: &[&str] = &["ollama", "lmstudio", "claude"];
pub const MAX_BLUR: u8 = 64;
pub const SHELLS: &[&str] = &["auto", "zsh", "bash", "fish"];
pub const PROMPT_THEMES: &[&str] = &["none", "two-line", "minimal", "blocks"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Prefs {
    pub theme: String,
    /// Window background opacity, 0.3–1.0.
    pub opacity: f32,
    /// Background blur radius behind the window, 0 = off (visible when opacity < 1).
    pub blur_radius: u8,
    pub font_size: u16,
    /// Prompt style injected by the shell integration ("none" keeps the user's own).
    pub prompt_theme: String,
    /// Truecolor gradient across the powerline blocks.
    pub prompt_gradient: bool,
    pub sidebar: bool,
    pub sidebar_width: u16,
    /// Shell for new tabs/panes: auto ($SHELL) | zsh | bash | fish.
    pub shell: String,
    pub ai_provider: String,
    /// Empty = provider default from config.toml.
    pub ai_model: String,
    /// Old on/off blur setting (before blur_radius); read once, never written.
    #[serde(rename = "blur", skip_serializing)]
    pub legacy_blur: Option<bool>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: "tokyo-night".into(),
            opacity: 1.0,
            blur_radius: 0,
            font_size: 0, // 0 = use config.toml
            prompt_theme: "blocks".into(),
            prompt_gradient: false,
            sidebar: true,
            sidebar_width: 220,
            shell: "auto".into(),
            ai_provider: "ollama".into(),
            ai_model: String::new(),
            legacy_blur: None,
        }
    }
}

impl Prefs {
    /// Clamp/whitelist everything coming from the WebView.
    pub fn sanitized(mut self, cloud: bool) -> Self {
        let d = Prefs::default();
        if !self.opacity.is_finite() {
            self.opacity = 1.0;
        }
        self.opacity = self.opacity.clamp(0.3, 1.0);
        if self.legacy_blur.take() == Some(true) && self.blur_radius == 0 {
            self.blur_radius = 24;
        }
        self.blur_radius = self.blur_radius.min(MAX_BLUR);
        self.sidebar_width = self.sidebar_width.clamp(160, 480);
        if self.font_size != 0 {
            self.font_size = self.font_size.clamp(9, 28);
        }
        if !SHELLS.contains(&self.shell.as_str()) {
            self.shell = d.shell.clone();
        }
        if !PROMPT_THEMES.contains(&self.prompt_theme.as_str()) {
            self.prompt_theme = d.prompt_theme;
        }
        if !PROVIDERS.contains(&self.ai_provider.as_str()) || (self.ai_provider == "claude" && !cloud) {
            self.ai_provider = d.ai_provider;
        }
        if self.theme.len() > 40 || !self.theme.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            self.theme = d.theme;
        }
        if self.ai_model.len() > 200 || self.ai_model.chars().any(|c| c.is_control()) {
            self.ai_model.clear();
        }
        self
    }
}

/// Contents of the file the shell reads before each prompt (see mirrorshade.zsh).
/// Returns None if `colors` isn't exactly 16 `#rrggbb` values.
pub fn prompt_state(p: &Prefs, colors: &[String]) -> Option<String> {
    let valid = colors.len() == 16
        && colors.iter().all(|c| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit()));
    if !valid {
        return None;
    }
    Some(format!(
        "theme={}\ngradient={}\ncolors={}\n",
        p.prompt_theme,
        p.prompt_gradient as u8,
        colors.join(",")
    ))
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

pub fn load(data_dir: &Path, cloud: bool) -> Prefs {
    std::fs::read_to_string(path(data_dir))
        .ok()
        .and_then(|t| serde_json::from_str::<Prefs>(&t).ok())
        .unwrap_or_default()
        .sanitized(cloud)
}

pub fn save(data_dir: &Path, p: &Prefs) {
    if let Ok(body) = serde_json::to_string_pretty(p) {
        let file = path(data_dir);
        let _ = std::fs::write(&file, body);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_untrusted_values() {
        let p = Prefs {
            theme: "../../etc".into(),
            opacity: 7.0,
            font_size: 200,
            blur_radius: 255,
            sidebar_width: 5,
            prompt_theme: "rm -rf".into(),
            ai_provider: "claude".into(),
            ai_model: "x\u{1b}[31m".into(),
            ..Prefs::default()
        }
        .sanitized(false);
        assert_eq!(p.theme, "tokyo-night");
        assert_eq!(p.opacity, 1.0);
        assert_eq!(p.font_size, 28);
        assert_eq!(p.blur_radius, MAX_BLUR);
        assert_eq!(p.sidebar_width, 160);
        assert_eq!(p.prompt_theme, "blocks");
        assert_eq!(p.ai_provider, "ollama"); // no cloud in this build
        assert!(p.ai_model.is_empty());
    }

    #[test]
    fn prompt_state_validates_colors() {
        let p = Prefs { prompt_gradient: true, ..Prefs::default() };
        let good: Vec<String> = (0..16).map(|i| format!("#{i:02x}aabb")).collect();
        let s = prompt_state(&p, &good).unwrap();
        assert!(s.starts_with("theme=blocks\ngradient=1\ncolors=#00aabb,"));
        let mut bad = good.clone();
        bad[3] = "#12345g".into();
        assert!(prompt_state(&p, &bad).is_none());
        bad[3] = "red\ntheme=x".into();
        assert!(prompt_state(&p, &bad).is_none());
        assert!(prompt_state(&p, &good[..15]).is_none());
    }

    #[test]
    fn migrates_old_blur_flag() {
        let p: Prefs = serde_json::from_str(r#"{"opacity":0.64,"blur":true}"#).unwrap();
        let p = p.sanitized(false);
        assert_eq!(p.blur_radius, 24);
        assert!(!serde_json::to_string(&p).unwrap().contains("\"blur\""));
    }
}
