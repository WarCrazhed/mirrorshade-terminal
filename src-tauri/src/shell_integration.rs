//! zsh integration: we point ZDOTDIR at a directory of our own startup files,
//! which source the user's real ones and then install the Mirrorshade hooks
//! (see `shell/mirrorshade.zsh`). Same approach as VS Code's terminal integration.

use std::io::Read;
use std::path::{Path, PathBuf};

const FILES: &[(&str, &str)] = &[
    ("zsh/.zshenv", include_str!("../shell/zshenv.zsh")),
    ("zsh/.zprofile", include_str!("../shell/zprofile.zsh")),
    ("zsh/.zshrc", include_str!("../shell/zshrc.zsh")),
    ("zsh/.zlogin", include_str!("../shell/zlogin.zsh")),
    ("zsh/mirrorshade.zsh", include_str!("../shell/mirrorshade.zsh")),
    ("bash/mirrorshade.bash", include_str!("../shell/mirrorshade.bash")),
    ("fish/mirrorshade.fish", include_str!("../shell/mirrorshade.fish")),
];

/// Write the integration scripts into `<data_dir>/shell-integration/`
/// (rewritten on every launch so they always match this app version).
/// Returns that root directory.
pub fn install(data_dir: &Path) -> std::io::Result<PathBuf> {
    let root = data_dir.join("shell-integration");
    for sub in ["", "zsh", "bash", "fish"] {
        let dir = root.join(sub);
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    for (name, body) in FILES {
        std::fs::write(root.join(name), body)?;
    }
    Ok(root)
}

/// 128-bit random hex nonce from the OS CSPRNG.
pub fn nonce() -> String {
    let mut buf = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut buf);
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// "zsh" | "bash" | "fish" for shells we integrate with.
pub fn shell_kind(shell: &str) -> Option<&'static str> {
    match Path::new(shell).file_name().and_then(|n| n.to_str())? {
        "zsh" => Some("zsh"),
        "bash" => Some("bash"),
        "fish" => Some("fish"),
        _ => None,
    }
}
