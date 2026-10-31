//! Stateless conversation shell policy shared by independent runtime owners.
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
pub(crate) fn sandbox(
    root: &Path,
    access: &crate::workbench::capabilities::ConversationCapabilities,
    network: bool,
) -> Value {
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
            "denyWrite": if access.shell_write { vec![] } else { vec![root.clone()] },
            "allowWrite": if access.shell_write { vec![root.clone()] } else { vec![] },
        },
        "network": {
            "allowedDomains": if !network { vec![] } else if access.command_domains.is_empty() { vec!["*".to_string()] } else { access.command_domains.clone() },
            "strictAllowlist": true,
            "allowLocalBinding": false,
            "allowUnixSockets": [],
            "allowAllUnixSockets": false,
        },
    })
}

/// Appended to the assistant instructions when the shell is offered.
pub(crate) fn instructions(root: &Path, writable: bool, network: bool) -> String {
    format!(
        "Bash runs in a sandbox rooted at {}. {} {} Use project_read, project_edit and project_create for recoverable file changes. Run host research jobs through authorized research profiles. Never write outside the supplied roots.",
        root.display(),
        if writable { "Commands may write in this isolated working copy; changes stay there until accepted." } else { "The project folder is read-only to commands; only scratch and temporary files are writable." },
        if network { "Command internet access follows the conversation domain policy." } else { "Commands have no internet access." }
    )
}
