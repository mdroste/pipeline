//! Claude Code owns the sandboxed Bash lifecycle; shared policy is stateless.
pub(super) use super::super::shell_policy::{instructions, sandbox};

/// Whether this platform can start the sandbox. Claude Code's sandbox uses
/// Seatbelt on macOS and bubblewrap with socat on Linux; native Windows has none.
pub(super) fn available() -> bool {
    if cfg!(target_os = "macos") {
        true
    } else if cfg!(target_os = "linux") {
        ["bwrap", "socat"]
            .iter()
            .all(|tool| crate::deps::resolve_command(tool).is_some())
    } else {
        super::wsl::detect().is_some()
    }
}
