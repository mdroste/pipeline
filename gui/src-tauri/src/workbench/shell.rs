//! Optional, Workspace-owned command engine for direct API providers. Only an
//! explicit installation command downloads a pinned engine. Each call receives
//! immutable sandbox policy, a clean environment and a bounded process group;
//! cancellation kills descendants. No subscription or Review process is used.
use super::store::{WorkbenchError, WorkbenchResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::AsyncReadExt;

const VERSION: &str = "0.0.81";
const PACKAGE: &str = "node_modules/@anthropic-ai/sandbox-runtime";
fn root() -> Result<PathBuf, String> {
    Ok(crate::storage::local_root()?.join("native/conversation-shell"))
}
fn err(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::invalid(e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellStatus {
    installed: bool,
    node_available: bool,
    platform_available: bool,
    version: &'static str,
}

#[tauri::command]
pub fn workbench_shell_status() -> Result<ShellStatus, String> {
    let root = root()?;
    let package = crate::safety::open_regular_file(&root.join(PACKAGE).join("package.json"))
        .ok()
        .and_then(|file| {
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(64 * 1024).read_to_end(&mut bytes).ok()?;
            serde_json::from_slice::<Value>(&bytes).ok()
        });
    Ok(ShellStatus {
        installed: package.is_some_and(|v| v["version"] == VERSION)
            && root.join(PACKAGE).join("dist/cli.js").is_file(),
        node_available: crate::deps::resolve_command("node").is_some(),
        platform_available: cfg!(target_os = "macos")
            || cfg!(target_os = "linux")
                && ["bwrap", "socat"]
                    .iter()
                    .all(|name| crate::deps::resolve_command(name).is_some()),
        version: VERSION,
    })
}

#[tauri::command]
pub async fn workbench_shell_install() -> Result<ShellStatus, String> {
    static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = INSTALL.lock().await;
    if !workbench_shell_status()?.platform_available {
        return Err(
            "Sandboxed API commands require macOS, or Linux with bubblewrap and socat.".into(),
        );
    }
    tokio::task::spawn_blocking(|| {
        let root = root()?;
        crate::agent_runtime::codex::session::private_directory(&root)?;
        let npm = crate::deps::resolve_command("npm").ok_or("Install Node.js 22.12 or later")?;
        let mut command = npm.command([
            "install",
            "--prefix",
            root.to_str().ok_or("Invalid installation path")?,
            "--no-audit",
            "--no-fund",
            "--ignore-scripts",
            "--save-exact",
            &format!("@anthropic-ai/sandbox-runtime@{VERSION}"),
        ]);
        command
            .env("PATH", crate::env::full_path())
            .current_dir(&root);
        let output = crate::process::run_bounded_independent(
            &mut command,
            Duration::from_secs(180),
            64 * 1024,
        )?;
        if !output.status.success() {
            return Err(
                "Command engine installation failed. Check Node.js and internet access.".into(),
            );
        }
        workbench_shell_status()
    })
    .await
    .map_err(|e| e.to_string())?
}

pub(crate) fn available() -> bool {
    workbench_shell_status().is_ok_and(|s| s.installed && s.node_available && s.platform_available)
}

pub(crate) fn catalog() -> Value {
    json!({"name":"shell","description":"Run a command in the conversation sandbox (up to 120 seconds, 128 KiB output). The project is read-only unless this is an editable isolated working copy. Use project_edit/project_create for recoverable changes. Command networking follows Assistant settings. Never request secrets or escape the sandbox. Use authorized research execution profiles for long jobs.","inputSchema":{"type":"object","properties":{"command":{"type":"string","maxLength":32768}},"required":["command"],"additionalProperties":false}})
}

pub(crate) async fn call(session: &str, input: Value) -> WorkbenchResult<Value> {
    let status = workbench_shell_status().map_err(err)?;
    if !status.installed || !status.platform_available {
        return Err(err("Install the command engine in Assistant settings. API commands require macOS or Linux with bubblewrap and socat."));
    }
    let command = input["command"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 32768 && !s.contains('\0'))
        .ok_or_else(|| err("Provide a command through 32 KiB"))?
        .to_owned();
    let session = session.to_owned();
    let (cwd, policy) = super::commands::run_store(move |store| {
        let effective = super::research::resolve_harness(&store, &session)?;
        let cwd = store.runtime_root(&session)?;
        let policy =
            super::shell_policy::sandbox(&cwd, &effective.capabilities, effective.command_network);
        Ok((cwd, policy))
    })
    .await?;
    run(
        &root().map_err(err)?.join(PACKAGE).join("dist/cli.js"),
        &cwd,
        &policy,
        &command,
    )
    .await
}

struct Process {
    child: Option<tokio::process::Child>,
    pid: u32,
    stopped: bool,
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if !self.stopped {
                crate::commands::lifecycle::kill_independent_process(self.pid);
            }
            let pid = self.pid;
            let _ = child.start_kill();
            tokio::spawn(async move {
                let _ = child.wait().await;
                crate::commands::lifecycle::unregister_independent_child_pid(pid);
            });
        }
    }
}

async fn capture(mut reader: impl tokio::io::AsyncRead + Unpin) -> Result<(String, bool), String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let n = reader.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let keep = n.min((64 * 1024usize).saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..keep]);
        truncated |= keep < n;
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

async fn run(cli: &Path, cwd: &Path, policy: &Value, command_text: &str) -> WorkbenchResult<Value> {
    use std::io::Write;
    let mut config = tempfile::NamedTempFile::new().map_err(err)?;
    let mut config_body = json!({"filesystem":policy["filesystem"],"network":policy["network"]});
    config_body["network"]["deniedDomains"] = json!([]);
    config
        .write_all(&serde_json::to_vec(&config_body).map_err(err)?)
        .map_err(err)?;
    config.as_file().sync_all().map_err(err)?;
    let node = crate::deps::resolve_command("node").ok_or_else(|| err("Node.js is unavailable"))?;
    let mut command = node.command([
        cli.as_os_str(),
        std::ffi::OsStr::new("--settings"),
        config.path().as_os_str(),
        std::ffi::OsStr::new("--"),
        std::ffi::OsStr::new("/bin/bash"),
        std::ffi::OsStr::new("--noprofile"),
        std::ffi::OsStr::new("--norc"),
        std::ffi::OsStr::new("-c"),
        std::ffi::OsStr::new(command_text),
    ]);
    command
        .env_clear()
        .env("PATH", crate::env::full_path())
        .env("LANG", "en_US.UTF-8")
        .current_dir(cwd);
    if let Some(home) = dirs::home_dir() {
        command.env("HOME", home);
    }
    crate::process::configure_isolation(&mut command);
    let child = tokio::process::Command::from(command)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(err)?;
    let pid = child.id().ok_or_else(|| err("Command process has no ID"))?;
    crate::commands::lifecycle::register_independent_child_pid(pid);
    let mut process = Process {
        child: Some(child),
        pid,
        stopped: false,
    };
    let stopped = &mut process.stopped;
    let child = process.child.as_mut().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        let wait = async {
            let status = child.wait().await;
            // Stop background descendants immediately, before waiting for
            // their capture pipes. Never defer a PID-only kill until later.
            crate::commands::lifecycle::kill_independent_process(pid);
            *stopped = true;
            status
        };
        let (status, stdout, stderr) = tokio::join!(wait, capture(stdout), capture(stderr));
        Ok::<_, String>((status.map_err(|e| e.to_string())?, stdout?, stderr?))
    })
    .await
    .map_err(|_| err("Command timed out after 120 seconds"))?
    .map_err(err)?;
    let _ = process.child.as_mut().unwrap().wait().await;
    crate::commands::lifecycle::unregister_independent_child_pid(pid);
    process.child = None;
    Ok(
        json!({"exitCode":result.0.code(),"stdout":result.1.0,"stderr":result.2.0,"truncated":result.1.1||result.2.1}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires the pinned sandbox engine, OS sandbox, and unsandboxed test runner"]
    async fn real_engine_confines_commands_and_cancellation_kills_descendants() {
        let cli = PathBuf::from(
            std::env::var("PIPELINE_SANDBOX_PROBE_CLI")
                .expect("Set PIPELINE_SANDBOX_PROBE_CLI to the pinned dist/cli.js"),
        );
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().join("project");
        std::fs::create_dir(&cwd).unwrap();
        let secret = directory.path().join("secret");
        std::fs::write(&secret, "private-fixture").unwrap();
        std::fs::write(cwd.join("paper.txt"), "paper").unwrap();
        let access = super::super::capabilities::ConversationCapabilities::default();
        let mut policy = super::super::shell_policy::sandbox(&cwd, &access, false);
        policy["filesystem"]["denyRead"]
            .as_array_mut()
            .unwrap()
            .push(json!(secret));
        let result = run(
            &cli,
            &cwd,
            &policy,
            "cat paper.txt; cat ../secret; echo changed > paper.txt; env | sort",
        )
        .await
        .unwrap();
        assert!(
            result["stdout"].as_str().unwrap().contains("paper"),
            "{result}"
        );
        assert!(!result["stdout"]
            .as_str()
            .unwrap()
            .contains("private-fixture"));
        assert_eq!(
            std::fs::read_to_string(cwd.join("paper.txt")).unwrap(),
            "paper"
        );
        let denied = run(
            &cli,
            &cwd,
            &policy,
            "curl --max-time 5 --silent --show-error https://example.com",
        )
        .await
        .unwrap();
        assert_ne!(denied["exitCode"], 0, "{denied}");
        let mut writable = access;
        writable.shell_write = true;
        let policy = super::super::shell_policy::sandbox(&cwd, &writable, false);
        let result = run(&cli, &cwd, &policy, "echo revised > paper.txt")
            .await
            .unwrap();
        assert_eq!(result["exitCode"], 0, "{result}");
        assert_eq!(
            std::fs::read_to_string(cwd.join("paper.txt")).unwrap(),
            "revised\n"
        );
        let returned = run(
            &cli,
            &cwd,
            &policy,
            "echo returned; (sleep 2; echo leaked > after-exit) &",
        )
        .await
        .unwrap();
        assert!(returned["stdout"].as_str().unwrap().contains("returned"));
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(!cwd.join("after-exit").exists());
        let task = tokio::spawn({
            let cwd = cwd.clone();
            async move {
                run(
                    &cli,
                    &cwd,
                    &policy,
                    "echo started > started; (sleep 3; echo leaked > leaked) & wait",
                )
                .await
            }
        });
        tokio::time::timeout(Duration::from_secs(20), async {
            while !cwd.join("started").exists() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        let _ = task.await;
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(!cwd.join("leaked").exists());
    }
}
