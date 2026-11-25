//! User-created color themes, stored locally in `<AppData>/themes.json`
//! (mode 0600). Everything coming from the WebView is validated: ids are
//! `custom-[a-z0-9-]`, colors are `#rrggbb`, names are short plain text.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_THEMES: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub light: bool,
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub cursor: String,
    pub selection: String,
    pub ansi: Vec<String>,
}

fn is_hex(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit())
}

impl CustomTheme {
    pub fn validate(mut self) -> Result<Self, String> {
        let id_ok = self.id.len() <= 40
            && self.id.starts_with("custom-")
            && self.id.len() > "custom-".len()
            && self.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !id_ok {
            return Err("id de tema inválido".into());
        }
        self.name = self.name.trim().chars().filter(|c| !c.is_control()).take(40).collect();
        if self.name.is_empty() {
            return Err("El tema necesita un nombre".into());
        }
        if self.ansi.len() != 16 {
            return Err("Se necesitan 16 colores de terminal".into());
        }
        for c in [&self.bg, &self.fg, &self.accent, &self.cursor, &self.selection].into_iter().chain(self.ansi.iter()) {
            if !is_hex(c) {
                return Err(format!("Color inválido: {c}"));
            }
        }
        for c in [&mut self.bg, &mut self.fg, &mut self.accent, &mut self.cursor, &mut self.selection]
            .into_iter()
            .chain(self.ansi.iter_mut())
        {
            *c = c.to_lowercase();
        }
        Ok(self)
    }
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("themes.json")
}

/// Load saved themes, silently dropping any entry that fails validation.
pub fn load(data_dir: &Path) -> Vec<CustomTheme> {
    std::fs::read_to_string(path(data_dir))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<CustomTheme>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| t.validate().ok())
        .take(MAX_THEMES)
        .collect()
}

fn store(data_dir: &Path, list: &[CustomTheme]) -> Result<(), String> {
    let body = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    let file = path(data_dir);
    std::fs::write(&file, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Insert or replace (by id). Returns the updated list.
pub fn save(data_dir: &Path, theme: CustomTheme) -> Result<Vec<CustomTheme>, String> {
    let theme = theme.validate()?;
    let mut list = load(data_dir);
    match list.iter_mut().find(|t| t.id == theme.id) {
        Some(existing) => *existing = theme,
        None => {
            if list.len() >= MAX_THEMES {
                return Err(format!("Máximo {MAX_THEMES} temas propios"));
            }
            list.push(theme);
        }
    }
    store(data_dir, &list)?;
    Ok(list)
}

pub fn delete(data_dir: &Path, id: &str) -> Result<Vec<CustomTheme>, String> {
    let mut list = load(data_dir);
    list.retain(|t| t.id != id);
    store(data_dir, &list)?;
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> CustomTheme {
        CustomTheme {
            id: id.into(),
            name: "  Mi tema\u{7}  ".into(),
            light: false,
            bg: "#101010".into(),
            fg: "#EEEEEE".into(),
            accent: "#ff8800".into(),
            cursor: "#ffffff".into(),
            selection: "#333333".into(),
            ansi: (0..16).map(|i| format!("#{i:02x}{i:02x}{i:02x}")).collect(),
        }
    }

    #[test]
    fn validates_and_normalizes() {
        let t = sample("custom-mi-tema-1a2b").validate().unwrap();
        assert_eq!(t.name, "Mi tema");
        assert_eq!(t.fg, "#eeeeee");
        assert!(sample("../etc").validate().is_err());
        assert!(sample("custom-").validate().is_err());
        assert!(sample("custom-UPPER").validate().is_err());
        let mut bad = sample("custom-x");
        bad.ansi[5] = "red".into();
        assert!(bad.validate().is_err());
        let mut short = sample("custom-x");
        short.ansi.pop();
        assert!(short.validate().is_err());
        let mut noname = sample("custom-x");
        noname.name = " \n ".into();
        assert!(noname.validate().is_err());
    }

    #[test]
    fn save_update_delete_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ms-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir).is_empty());
        let list = save(&dir, sample("custom-a")).unwrap();
        assert_eq!(list.len(), 1);
        let mut edited = sample("custom-a");
        edited.name = "Editado".into();
        let list = save(&dir, edited).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Editado");
        save(&dir, sample("custom-b")).unwrap();
        assert_eq!(load(&dir).len(), 2);
        let list = delete(&dir, "custom-a").unwrap();
        assert_eq!(list.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["custom-b"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
