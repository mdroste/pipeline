//! Claude Code's Bash tool, confined by Claude Code's own OS sandbox: read-only
//! access to the conversation's working root, writes only to the turn's private
//! scratch folder and temporary files, and no network. Project edits still go
//! through `project_edit`; research jobs still need an authorized profile.
//!
//! Bash is never in `--allowedTools`. Under `dontAsk`, only the sandbox's own
//! auto-approval runs it, so a command that would leave the sandbox is denied.
//! The sandbox is the one key added to the subscription `--settings` value and
//! cannot replace its login or helper overrides. It covers Bash and its child
//! processes, not Claude Code's own process.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Credential and private-store folders stay unreadable even when a project
/// root contains them. A narrower sandbox rule wins over a broader one.
const SECRET_PATHS: [&str; 11] = [
    "~/.ssh",
    "~/.aws",
    "~/.azure",
    "~/.gnupg",
    "~/.netrc",
    "~/.config",
    "~/.docker",
    "~/.kube",
    "~/.claude",
    "~/.codex",
    "~/Library/Keychains",
];

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
        false
    }
}

/// Folders outside the root that a sandboxed command must not read: home and
/// user folders, mounted volumes, and Pipeline's private store.
fn denied_reads() -> Vec<String> {
    let mut denied = vec!["~/".to_string()];
    if cfg!(target_os = "macos") {
        denied.extend(["/Users", "/Volumes"].map(str::to_owned));
    } else {
        denied.extend(["/home", "/root", "/mnt", "/media"].map(str::to_owned));
    }
    let stores: [Result<PathBuf, String>; 2] =
        [crate::storage::local_root(), crate::storage::data_root()];
    denied.extend(
        stores
            .into_iter()
            .flatten()
            .map(|path| path.to_string_lossy().into_owned()),
    );
    denied
}

/// Settings for one turn. Starting fails rather than running unconfined, and
/// the model cannot ask to leave the sandbox.
pub(super) fn sandbox(root: &Path) -> Value {
    let root = root.to_string_lossy().into_owned();
    let mut denied = denied_reads();
    denied.extend(SECRET_PATHS.map(str::to_owned));
    json!({
        "enabled": true,
        "failIfUnavailable": true,
        "autoAllowBashIfSandboxed": true,
        "allowUnsandboxedCommands": false,
        "excludedCommands": [],
        "filesystem": {
            "denyRead": denied,
            "allowRead": [root],
            "denyWrite": [root],
        },
        "network": {
            "allowedDomains": [],
            "strictAllowlist": true,
            "allowLocalBinding": false,
            "allowUnixSockets": [],
            "allowAllUnixSockets": false,
        },
    })
}

/// Appended to the assistant instructions when the shell is offered.
pub(super) fn instructions(root: &Path) -> String {
    format!(
        "A Bash tool runs commands in a sandbox. It can read the project folder {}, write only to the current scratch folder and temporary files, and has no network access. Use it to inspect files or run quick computations. It cannot change project files: use project_edit for edits. Run research jobs through authorized research profiles.",
        root.display()
    )
}
