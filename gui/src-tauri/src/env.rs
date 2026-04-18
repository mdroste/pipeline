//! PATH resolution for GUI apps.
//!
//! macOS/Linux apps launched from a desktop environment may inherit a
//! minimal PATH that excludes Homebrew, nvm, cargo, and other
//! user-installed tool directories. We fix this once at startup by
//! asking the user's login shell for its PATH.
//!
//! On Windows, GUI apps inherit the full user PATH, so no fixup is needed.
//!
//! We also bundle `pdftoppm` and `pdftotext` (from poppler) inside the app.
//! At startup, `set_bundled_poppler_dir` is called with the resolved
//! resource path; that directory is prepended to PATH so subprocesses
//! (notably `claude -p`, which uses pdftoppm to render PDFs) can find them
//! without the user installing poppler separately.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static FULL_PATH: OnceLock<String> = OnceLock::new();
static BUNDLED_POPPLER_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Record the directory where bundled poppler binaries live for this build.
/// Called once at startup from the Tauri setup hook.  Subsequent calls are no-ops.
pub fn set_bundled_poppler_dir(dir: Option<PathBuf>) {
    let _ = BUNDLED_POPPLER_DIR.set(dir);
}

/// Return the bundled poppler directory if one was registered and `pdftoppm`
/// is actually present in it.  Used by deps detection to distinguish bundled
/// vs system installs.
pub fn bundled_poppler_dir() -> Option<&'static Path> {
    BUNDLED_POPPLER_DIR
        .get()
        .and_then(|opt| opt.as_deref())
        .filter(|dir| {
            let bin = if cfg!(windows) { "pdftoppm.exe" } else { "pdftoppm" };
            dir.join(bin).is_file()
        })
}

/// Return the user's full PATH, with the bundled poppler dir prepended.
///
/// The base PATH is resolved from the login shell on macOS/Linux when the
/// inherited PATH looks minimal (no homebrew/nvm/cargo entries).
pub fn full_path() -> String {
    let base = FULL_PATH.get_or_init(resolve_base_path);
    match bundled_poppler_dir() {
        Some(dir) => {
            let sep = if cfg!(windows) { ";" } else { ":" };
            format!("{}{sep}{base}", dir.display())
        }
        None => base.clone(),
    }
}

fn resolve_base_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();

    // Windows: GUI apps already have the full PATH.
    if cfg!(windows) {
        return current;
    }

    // If we already have a rich PATH (e.g. launched from a terminal), keep it.
    if current.contains(".nvm")
        || current.contains("homebrew")
        || current.contains("/opt/")
        || current.contains(".cargo")
    {
        return current;
    }

    // Ask the user's login shell for its PATH.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    if let Ok(output) = std::process::Command::new(&shell)
        .args(["-ilc", "echo $PATH"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }

    current
}
