//! Native Workspace command runner. Rust owns the filtered proxy and bounded
//! process lifecycle; the pinned MXC Rust library applies Seatbelt, Bubblewrap,
//! or Windows ProcessContainer policy. An unavailable sandbox is a hard error.
use super::store::{WorkbenchError, WorkbenchResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod policy;
mod process;
mod proxy;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellStatus {
    available: bool,
    reason: Option<String>,
}

#[tauri::command(async)]
pub fn workbench_shell_status() -> ShellStatus {
    let reason = unavailable_reason();
    ShellStatus {
        available: reason.is_none(),
        reason,
    }
}

fn unavailable_reason() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        None
    }
    #[cfg(target_os = "linux")]
    {
        if crate::deps::resolve_command("bwrap").is_none() {
            return Some("Install bubblewrap to use commands. Command internet access also needs slirp4netns, iproute2, iptables and util-linux.".into());
        }
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/fuse")
            .err()
            .map(|_| "Linux commands need FUSE support and permission to open /dev/fuse.".into())
    }
    #[cfg(windows)]
    {
        match mxc_sdk::v1::probe(None) {
            Ok(probe) if probe.tier == Some("base-container") && probe.probes.base_container_supports_deny_paths => None,
            _ => Some("Commands require an updated Windows 11 installation with native ProcessContainer support. Update Windows and check again.".into()),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Some("Commands are unavailable on this operating system.".into())
    }
}

pub(crate) fn available() -> bool {
    unavailable_reason().is_none()
}
pub(crate) fn cancel_all() {
    process::cancel_all();
}

pub(crate) fn run_guard_if_requested() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::run_if_requested()
    }
    #[cfg(windows)]
    {
        windows::run_if_requested()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        false
    }
}

pub(crate) fn catalog(private_folder: bool) -> Value {
    let description = if private_folder {
        "Run a command in this conversation's private working folder (up to 120 seconds, 128 KiB output). Commands can create and change files there, and those files stay available in later messages. Command networking follows Assistant settings. Never request secrets or escape the sandbox."
    } else {
        "Run a command in the conversation sandbox (up to 120 seconds, 128 KiB output). The project is read-only unless this is an editable isolated working copy. Use project_edit/project_create for recoverable changes. Command networking follows Assistant settings. Never request secrets or escape the sandbox. Use authorized research execution profiles for long jobs."
    };
    let description =
        format!("{description} Use Bash syntax on macOS/Linux and PowerShell syntax on Windows.");
    json!({"name":"shell","description":description,"inputSchema":{"type":"object","properties":{"command":{"type":"string","maxLength":32768}},"required":["command"],"additionalProperties":false}})
}

pub(crate) async fn call(session: &str, input: Value) -> WorkbenchResult<Value> {
    let command = input["command"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 32768 && !s.contains('\0'))
        .ok_or_else(|| WorkbenchError::invalid("Provide a command through 32 KiB"))?
        .to_owned();
    let session = session.to_owned();
    let (cwd, access, network) = super::commands::run_store(move |store| {
        let effective = super::research::resolve_harness(&store, &session)?;
        Ok((
            store.runtime_root(&session)?,
            effective.capabilities,
            effective.command_network,
        ))
    })
    .await?;
    run(
        &cwd,
        access.shell_write,
        &access.command_domains,
        network,
        &command,
    )
    .await
    .map_err(WorkbenchError::invalid)
}

async fn run(
    cwd: &Path,
    writable: bool,
    domains: &[String],
    network: bool,
    command: &str,
) -> Result<Value, String> {
    let domains = policy::domains(domains, network)?;
    if let Some(reason) = unavailable_reason() {
        return Err(reason);
    }
    let proxy = if network {
        Some(proxy::Proxy::start(domains).await?)
    } else {
        None
    };
    let scratch = tempfile::Builder::new()
        .prefix("pipeline-command-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let request = policy::request(cwd, writable, scratch.path(), command, proxy.as_ref())?;
    #[cfg(windows)]
    {
        // Never mutate host ACLs or accept the SDK's less isolated compatibility
        // tiers. Request-specific probing also checks filesystem restrictions.
        let probe = mxc_sdk::v1::probe(Some(&request)).map_err(|e| e.to_string())?;
        if probe.tier != Some("base-container") || probe.error.is_some() {
            return Err(probe.error.unwrap_or_else(|| "Windows cannot enforce this command's restrictions. Update Windows before trying again.".into()));
        }
    }
    process::run(request, scratch, proxy).await
}
