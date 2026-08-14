//! PATH resolution for GUI apps.
//!
//! macOS/Linux apps launched from a desktop environment may inherit a
//! minimal PATH that excludes Homebrew, nvm, cargo, and other
//! user-installed tool directories. We fix this by asking the user's
//! login shell for its PATH.
//!
//! On Windows, GUI apps inherit the full user PATH, so no fixup is needed.
//!
//! We also bundle `pdftoppm` and `pdftotext` (from poppler) inside the app.
//! At startup, `set_bundled_poppler_dir` is called with the resolved
//! resource path; that directory is prepended to PATH so subprocesses
//! (notably `claude -p`, which uses pdftoppm to render PDFs) can find them
//! without the user installing poppler separately.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

/// Successful base-PATH resolution, cached for the app session. A failed
/// probe is retried, but only after `PROBE_RETRY_INTERVAL`: each probe costs
/// up to 5 s on a runtime worker, and provider spawns call `full_path()`
/// several times, so an unretried failure would stall every step spawn for
/// the whole session when the user's shell startup hangs or prints garbage.
static FULL_PATH: RwLock<Option<String>> = RwLock::new(None);
/// Timestamp of the most recent failed login-shell probe. The mutex also
/// single-flights the probe itself: it is held across `resolve_base_path`.
static PROBE_FAILED_AT: Mutex<Option<Instant>> = Mutex::new(None);
const PROBE_RETRY_INTERVAL: Duration = Duration::from_secs(300);
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
            let bin = if cfg!(windows) {
                "pdftoppm.exe"
            } else {
                "pdftoppm"
            };
            dir.join(bin).is_file()
        })
}

/// Return the user's full PATH, with the bundled poppler dir prepended.
///
/// The base PATH is resolved from the login shell on macOS/Linux when the
/// inherited PATH carries no user-owned tool directories.
pub fn full_path() -> String {
    let base = cached_base_path();
    match bundled_poppler_dir() {
        Some(dir) => {
            let sep = if cfg!(windows) { ";" } else { ":" };
            format!("{}{sep}{base}", dir.display())
        }
        None => base,
    }
}

fn cached_base_path() -> String {
    if let Some(cached) = FULL_PATH.read().ok().and_then(|guard| guard.clone()) {
        return cached;
    }
    // Single-flight: if another call is already probing (or the mutex is
    // poisoned), use the inherited PATH now rather than queueing up to 5 s
    // behind the in-flight probe.
    let Ok(mut failed_at) = PROBE_FAILED_AT.try_lock() else {
        return std::env::var("PATH").unwrap_or_default();
    };
    // The probe that held the lock may have just succeeded.
    if let Some(cached) = FULL_PATH.read().ok().and_then(|guard| guard.clone()) {
        return cached;
    }
    if failed_at.is_some_and(|at| at.elapsed() < PROBE_RETRY_INTERVAL) {
        return std::env::var("PATH").unwrap_or_default();
    }
    match resolve_base_path() {
        Some(resolved) => {
            *failed_at = None;
            if let Ok(mut guard) = FULL_PATH.write() {
                guard.get_or_insert_with(|| resolved.clone());
            }
            resolved
        }
        // Probe failed: use the inherited PATH until the retry interval
        // elapses, then retry once per interval.
        None => {
            *failed_at = Some(Instant::now());
            std::env::var("PATH").unwrap_or_default()
        }
    }
}

fn resolve_base_path() -> Option<String> {
    let current = std::env::var("PATH").unwrap_or_default();

    // Windows: GUI apps already have the full PATH.
    if cfg!(windows) {
        return Some(current);
    }

    // A PATH that already carries user-owned tool directories came from a
    // terminal or login environment; keep it. Only home-directory entries and
    // Homebrew's own prefix count: system-provided /opt/... entries (e.g.
    // /etc/profile.d additions on Linux desktops) say nothing about where the
    // user's toolchain lives.
    if path_is_user_configured(&current) {
        return Some(current);
    }

    // Ask the user's login shell for its PATH, then keep any inherited
    // entries it does not know about.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let probed = probe_login_shell_path(&shell)?;
    Some(merge_paths(&probed, &current))
}

fn path_is_user_configured(path: &str) -> bool {
    let home = dirs::home_dir();
    path.split(':').any(|entry| {
        entry.starts_with("/opt/homebrew")
            || home
                .as_ref()
                .is_some_and(|home| Path::new(entry).starts_with(home))
    })
}

fn probe_login_shell_path(shell: &str) -> Option<String> {
    let shell_name = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sh");
    // `printenv PATH` prints the colon-separated variable under every shell;
    // fish would join `echo $PATH` with spaces. Interactive rc files may
    // print banners first, so only the last non-empty stdout line is taken
    // as the PATH (see parse_probe_stdout).
    let mut command = std::process::Command::new(shell);
    match shell_name {
        // tcsh/csh honor -l only as the sole flag; grouped or extra flags
        // make them exit with a usage error.
        "csh" | "tcsh" => command.args(["-i", "-c", "printenv PATH"]),
        // nushell has no -i; -l loads the login environment.
        "nu" | "nushell" => command.args(["-l", "-c", "printenv PATH"]),
        _ => command.args(["-i", "-l", "-c", "printenv PATH"]),
    };
    let output = crate::process::run_bounded(
        &mut command,
        std::time::Duration::from_secs(5),
        64 * 1024,
    )
    .ok()?;
    if !output.status.success() || output.stdout_truncated || output.stderr_truncated {
        return None;
    }
    parse_probe_stdout(&String::from_utf8_lossy(&output.stdout))
}

/// Extract the PATH from login-shell probe output. Interactive rc files may
/// print greetings or tool notices before the probe command runs, so the
/// PATH is the last non-empty line — and it must contain at least one
/// absolute entry, which a stray banner line does not.
fn parse_probe_stdout(stdout: &str) -> Option<String> {
    let path = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .to_string();
    if !path.split(':').any(|entry| entry.starts_with('/')) {
        return None;
    }
    Some(path)
}

/// Primary entries first, then any fallback entries not already present.
fn merge_paths(primary: &str, fallback: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut merged = Vec::new();
    for entry in primary.split(':').chain(fallback.split(':')) {
        if entry.is_empty() {
            continue;
        }
        if seen.insert(entry.to_string()) {
            merged.push(entry.to_string());
        }
    }
    merged.join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_output_takes_last_non_empty_line() {
        assert_eq!(
            parse_probe_stdout("Welcome to my shell!\n/opt/homebrew/bin:/usr/bin:/bin\n"),
            Some("/opt/homebrew/bin:/usr/bin:/bin".to_string())
        );
        assert_eq!(
            parse_probe_stdout("nvm: using node v22\n\n/Users/x/.nvm/versions/node/v22/bin:/usr/bin\n\n"),
            Some("/Users/x/.nvm/versions/node/v22/bin:/usr/bin".to_string())
        );
    }

    #[test]
    fn probe_output_rejects_banner_only_output() {
        assert_eq!(parse_probe_stdout("Welcome!\n"), None);
        assert_eq!(parse_probe_stdout(""), None);
        assert_eq!(parse_probe_stdout("Logged in from tty1\n"), None);
    }

    #[test]
    fn merge_keeps_primary_order_and_appends_unknown_fallback_entries() {
        assert_eq!(
            merge_paths("/a:/b", "/b:/c:"),
            "/a:/b:/c".to_string()
        );
    }

    #[test]
    fn system_opt_entries_do_not_count_as_user_configured() {
        // Linux desktops commonly add /opt/<vendor> via /etc/profile.d while
        // the user's toolchain lives under $HOME and is added only by rc
        // files; such a PATH must still trigger the login-shell probe.
        assert!(!path_is_user_configured("/usr/bin:/bin:/opt/google/chrome"));
        assert!(path_is_user_configured("/usr/bin:/opt/homebrew/bin"));
        if let Some(home) = dirs::home_dir() {
            let entry = home.join(".local/bin");
            assert!(path_is_user_configured(&format!(
                "/usr/bin:{}",
                entry.display()
            )));
        }
    }
}
