//! PATH resolution for GUI apps.
//!
//! macOS/Linux apps launched from a desktop environment may inherit a
//! minimal PATH that excludes Homebrew, nvm, cargo, and other
//! user-installed tool directories. We fix this once at startup by
//! asking the user's login shell for its PATH.
//!
//! On Windows, GUI apps inherit the full user PATH, so no fixup is needed.

use std::sync::OnceLock;

static FULL_PATH: OnceLock<String> = OnceLock::new();

/// Return the user's full PATH, resolving it from the login shell if needed.
pub fn full_path() -> &'static str {
    FULL_PATH.get_or_init(|| {
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
    })
}
