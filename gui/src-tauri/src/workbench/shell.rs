//! Workspace-owned API command engine, provisioned lazily with a private runtime.
//! Each call keeps sandbox policy, bounded output, and descendant cancellation.
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
    super::tool_setup::root()
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
    setup_phase: Option<String>,
}

#[tauri::command(async)]
pub fn workbench_shell_status() -> Result<ShellStatus, String> {
    Ok(ShellStatus {
        installed: super::tool_setup::ready() && available(),
        node_available: super::tool_setup::ready(),
        platform_available: available(),
        version: VERSION,
        setup_phase: super::tool_setup::phase(),
    })
}

#[tauri::command]
pub async fn workbench_shell_install() -> Result<ShellStatus, String> {
    if !available() {
        return Err("Commands require macOS, or Linux with bubblewrap and socat.".into());
    }
    super::tool_setup::ensure().await?;
    workbench_shell_status()
}

pub(crate) fn available() -> bool {
    super::tool_setup::supported()
        && (cfg!(target_os = "macos")
            || cfg!(target_os = "linux")
                && ["bwrap", "socat"]
                    .iter()
                    .all(|name| crate::deps::resolve_command(name).is_some()))
}

pub(crate) fn catalog(private_folder: bool) -> Value {
    let description = if private_folder {
        "Run a command in this conversation's private working folder (up to 120 seconds, 128 KiB output). Commands can create and change files there, and those files stay available in later messages. Command networking follows Assistant settings. Never request secrets or escape the sandbox."
    } else {
        "Run a command in the conversation sandbox (up to 120 seconds, 128 KiB output). The project is read-only unless this is an editable isolated working copy. Use project_edit/project_create for recoverable changes. Command networking follows Assistant settings. Never request secrets or escape the sandbox. Use authorized research execution profiles for long jobs."
    };
    json!({"name":"shell","description":description,"inputSchema":{"type":"object","properties":{"command":{"type":"string","maxLength":32768}},"required":["command"],"additionalProperties":false}})
}

pub(crate) async fn call(session: &str, input: Value) -> WorkbenchResult<Value> {
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
    // A command that cannot run must not cost a first-time engine download.
    check_policy(&policy)?;
    workbench_shell_install().await.map_err(err)?;
    run(
        &super::tool_setup::node().map_err(err)?,
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

const SITES_REQUIRED: &str = "This command did not run. API and local server conversations need a list of sites for command internet access. List them under Allowed command sites in Assistant settings, for example pypi.org, or turn off Internet access for commands.";
const POLICY_INVALID: &str =
    "This command did not run because its sandbox settings could not be checked.";
const SANDBOX_FAILED: &str =
    "The command sandbox could not start, so Pipeline discarded this command's result.";

/// The engine and the command share stderr, so wording alone cannot tell them
/// apart: many tools print "Invalid configuration ...". The engine names the
/// settings file it refused, and that path is private to this launch. This is
/// only a backstop; `check_policy` is what keeps a refused policy from starting.
fn engine_refused_settings(stderr: &str, settings: &Path) -> bool {
    stderr.contains(&*settings.to_string_lossy())
}

/// A site the engine's settings schema accepts. It refuses a bare `*`, and as
/// far as is known it then runs under its defaults rather than stopping, so a
/// settings file it would refuse must never reach a launch.
fn accepted_site(site: &Value) -> bool {
    site.as_str().is_some_and(|site| {
        let host = site.strip_prefix("*.").unwrap_or(site);
        host.contains('.') && !host.contains('*')
    })
}

fn check_policy(policy: &Value) -> WorkbenchResult<()> {
    let filesystem = &policy["filesystem"];
    let sites = policy["network"]["allowedDomains"].as_array();
    let denied = filesystem["denyRead"].as_array();
    if sites.is_none()
        || denied.is_none_or(Vec::is_empty)
        || !["allowRead", "denyWrite", "allowWrite"]
            .iter()
            .all(|key| filesystem[key].is_array())
    {
        return Err(err(POLICY_INVALID));
    }
    if sites.is_some_and(|sites| !sites.iter().all(accepted_site)) {
        return Err(err(SITES_REQUIRED));
    }
    Ok(())
}

async fn run(
    node: &Path,
    cli: &Path,
    cwd: &Path,
    policy: &Value,
    command_text: &str,
) -> WorkbenchResult<Value> {
    check_policy(policy)?;
    launch(node, cli, cwd, policy, command_text).await
}

async fn launch(
    node: &Path,
    cli: &Path,
    cwd: &Path,
    policy: &Value,
    command_text: &str,
) -> WorkbenchResult<Value> {
    use std::io::Write;
    let mut config = tempfile::NamedTempFile::new().map_err(err)?;
    let mut config_body = json!({"filesystem":policy["filesystem"],"network":policy["network"]});
    config_body["network"]["deniedDomains"] = json!([]);
    config
        .write_all(&serde_json::to_vec(&config_body).map_err(err)?)
        .map_err(err)?;
    config.as_file().sync_all().map_err(err)?;
    let mut command = std::process::Command::new(node);
    command.args([
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
    // The command may have run outside its policy, so nothing it printed leaves here.
    if engine_refused_settings(&result.2 .0, config.path()) {
        return Err(err(SANDBOX_FAILED));
    }
    Ok(
        json!({"exitCode":result.0.code(),"stdout":result.1.0,"stderr":result.2.0,"truncated":result.1.1||result.2.1}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(domains: &[&str]) -> super::super::capabilities::ConversationCapabilities {
        super::super::capabilities::ConversationCapabilities {
            command_domains: domains.iter().map(|site| (*site).into()).collect(),
            ..Default::default()
        }
    }

    /// A stand-in engine, started the way the real one is: `node cli --settings
    /// <file> -- /bin/bash ...`, with /bin/sh playing the part of node.
    #[cfg(unix)]
    fn fake_engine(directory: &Path, body: &str) -> PathBuf {
        let path = directory.join("engine.sh");
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn claude_code_keeps_blank_as_every_site_and_the_api_engine_refuses_it() {
        let root = Path::new("/project");
        let blank = super::super::shell_policy::sandbox(root, &access(&[]), true);
        assert_eq!(blank["network"]["allowedDomains"], json!(["*"]));
        let error = check_policy(&blank).unwrap_err();
        assert!(error.message.contains("Allowed command sites"));
        for (network, sites) in [(false, &[][..]), (true, &["pypi.org", "*.example.org"][..])] {
            let policy = super::super::shell_policy::sandbox(root, &access(sites), network);
            check_policy(&policy).unwrap();
        }
    }

    #[test]
    fn a_policy_the_engine_would_refuse_never_passes_the_check() {
        let root = Path::new("/project");
        let good = super::super::shell_policy::sandbox(root, &access(&[]), false);
        let mut rejected = vec![json!(null), json!({"filesystem":good["filesystem"]})];
        for site in ["*", "*.com", "com", "exa*mple.org"] {
            let mut policy = good.clone();
            policy["network"]["allowedDomains"] = json!([site]);
            rejected.push(policy);
        }
        for key in ["denyRead", "allowRead", "denyWrite", "allowWrite"] {
            let mut policy = good.clone();
            policy["filesystem"][key] = Value::Null;
            rejected.push(policy);
        }
        let mut open = good.clone();
        open["filesystem"]["denyRead"] = json!([]);
        rejected.push(open);
        for policy in rejected {
            assert!(check_policy(&policy).is_err(), "{policy}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn blank_sites_with_network_on_are_refused_before_anything_starts() {
        let directory = tempfile::tempdir().unwrap();
        let started = directory.path().join("started");
        let engine = fake_engine(
            directory.path(),
            &format!("echo ran > '{}'\n", started.display()),
        );
        let policy = super::super::shell_policy::sandbox(directory.path(), &access(&[]), true);
        let error = run(
            Path::new("/bin/sh"),
            &engine,
            directory.path(),
            &policy,
            "cat secret",
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("did not run"), "{}", error.message);
        assert!(error.message.contains("Allowed command sites"));
        assert!(!started.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn listed_sites_reach_the_engine_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let engine = fake_engine(directory.path(), "cat \"$2\"\n");
        let policy =
            super::super::shell_policy::sandbox(directory.path(), &access(&["pypi.org"]), true);
        let result = run(
            Path::new("/bin/sh"),
            &engine,
            directory.path(),
            &policy,
            "true",
        )
        .await
        .unwrap();
        let settings: Value = serde_json::from_str(result["stdout"].as_str().unwrap()).unwrap();
        assert_eq!(settings["network"]["allowedDomains"], json!(["pypi.org"]));
        assert_eq!(settings["network"]["deniedDomains"], json!([]));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_engine_that_names_its_refused_settings_file_is_an_error_with_no_output() {
        for complaint in [
            "Invalid configuration in $2:",
            "Invalid JSON in config file $2: Unexpected token",
            "Failed to load config from $2",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let engine = fake_engine(
                directory.path(),
                &format!("echo \"{complaint}\" >&2\necho private-fixture\nexit 0\n"),
            );
            let policy = super::super::shell_policy::sandbox(directory.path(), &access(&[]), false);
            let error = run(
                Path::new("/bin/sh"),
                &engine,
                directory.path(),
                &policy,
                "cat secret",
            )
            .await
            .unwrap_err();
            assert!(error.message.contains("sandbox could not start"));
            assert!(!error.message.contains("private-fixture"));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_command_that_prints_startup_failure_wording_keeps_its_result() {
        for line in [
            "Invalid configuration object. Webpack has been initialized with a bad option.",
            "Invalid JSON in config file package.json",
            "Failed to load config from ./app.toml",
            "Error: Failed to initialize sandbox",
            "Error: Sandbox dependencies not available: bubblewrap",
            "stata: Invalid configuration of the do-file",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let engine = fake_engine(
                directory.path(),
                &format!("echo '{line}' >&2\necho results\nexit 3\n"),
            );
            let policy = super::super::shell_policy::sandbox(directory.path(), &access(&[]), false);
            let result = run(
                Path::new("/bin/sh"),
                &engine,
                directory.path(),
                &policy,
                "true",
            )
            .await
            .unwrap();
            assert_eq!(result["exitCode"], 3, "{line}");
            assert!(result["stdout"].as_str().unwrap().contains("results"));
            assert!(result["stderr"].as_str().unwrap().contains(line));
        }
    }

    #[tokio::test]
    #[ignore = "Requires the pinned sandbox engine, OS sandbox, and unsandboxed test runner"]
    async fn real_engine_never_returns_a_denied_file_when_handed_a_bare_wildcard() {
        let (_runtime, runtime_root) = super::super::tool_setup::fixture_runtime().await;
        let node = runtime_root.join("runtime/bin/node");
        let cli = runtime_root.join(PACKAGE).join("dist/cli.js");
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().join("project");
        std::fs::create_dir(&cwd).unwrap();
        let secret = directory.path().join("secret");
        std::fs::write(&secret, "private-fixture").unwrap();
        let mut policy = super::super::shell_policy::sandbox(&cwd, &access(&[]), true);
        policy["filesystem"]["denyRead"]
            .as_array_mut()
            .unwrap()
            .push(json!(secret));
        let refused = run(&node, &cli, &cwd, &policy, "cat ../secret").await;
        assert!(refused
            .unwrap_err()
            .message
            .contains("Allowed command sites"));
        // Past the pre-launch check, this shows what the engine does with a
        // list its schema refuses. Run with --nocapture to read the outcome.
        let launched = launch(&node, &cli, &cwd, &policy, "cat ../secret; echo done").await;
        println!("engine outcome with a bare wildcard: {launched:?}");
        match launched {
            Ok(result) => assert!(!result.to_string().contains("private-fixture"), "{result}"),
            Err(error) => assert!(!error.message.contains("private-fixture")),
        }
    }

    #[tokio::test]
    #[ignore = "Requires the pinned sandbox engine, OS sandbox, and unsandboxed test runner"]
    async fn real_engine_writes_in_a_private_folder_beneath_denied_home_reads() {
        let (_runtime, runtime_root) = super::super::tool_setup::fixture_runtime().await;
        let node = runtime_root.join("runtime/bin/node");
        let cli = runtime_root.join(PACKAGE).join("dist/cli.js");
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
        let directory = tempfile::Builder::new()
            .prefix(".pipeline-folder-probe-")
            .tempdir_in(home)
            .unwrap();
        let cwd = directory.path().join("jobs/conversations/session");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::write(directory.path().join("secret"), "private-fixture").unwrap();
        let access = super::super::capabilities::ConversationCapabilities {
            shell_write: true,
            private_folder: true,
            ..Default::default()
        };
        let policy = super::super::shell_policy::sandbox(&cwd, &access, false);
        let result = run(
            &node,
            &cli,
            &cwd,
            &policy,
            "printf made > script.py; cat script.py; cat ../../../secret; echo escaped > ../outside",
        )
        .await
        .unwrap();
        let stdout = result["stdout"].as_str().unwrap();
        assert!(stdout.contains("made"), "{result}");
        assert!(!stdout.contains("private-fixture"), "{result}");
        assert_eq!(
            std::fs::read_to_string(cwd.join("script.py")).unwrap(),
            "made"
        );
        assert!(!cwd.parent().unwrap().join("outside").exists());
    }

    #[tokio::test]
    #[ignore = "Requires the pinned sandbox engine, OS sandbox, and unsandboxed test runner"]
    async fn real_engine_confines_commands_and_cancellation_kills_descendants() {
        let (_runtime, runtime_root) = super::super::tool_setup::fixture_runtime().await;
        let node = runtime_root.join(if cfg!(windows) {
            "runtime/node.exe"
        } else {
            "runtime/bin/node"
        });
        let cli = runtime_root.join(PACKAGE).join("dist/cli.js");
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
            &node,
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
            &node,
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
        let result = run(&node, &cli, &cwd, &policy, "echo revised > paper.txt")
            .await
            .unwrap();
        assert_eq!(result["exitCode"], 0, "{result}");
        assert_eq!(
            std::fs::read_to_string(cwd.join("paper.txt")).unwrap(),
            "revised\n"
        );
        let returned = run(
            &node,
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
                    &node,
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
