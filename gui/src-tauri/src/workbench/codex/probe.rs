use super::compatibility::{
    detected_version, DetectedCodexVersion, EXPERIMENTAL_SCHEMA_SHA256, STABLE_SCHEMA_SHA256,
};
use base64::Engine as _;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};

mod config;
mod discovery;
use config::write_probe_config;

const PROBE_SCHEMA_VERSION: u32 = 1;
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const VERSION_OUTPUT_LIMIT: usize = 64 * 1024;
const FRAME_LIMIT: usize = 32 * 1024 * 1024;
const STDERR_LIMIT: usize = 4 * 1024 * 1024;
const MAX_FRAMES_PER_RESPONSE: usize = 128;
const PROBE_PERMISSION_PROFILE: &str = "pipeline-workbench-inspect";
const TERMINATED_COMMAND_PROCESS_ID: &str = "pipeline-workbench-terminate-probe";
const STDIN_COMMAND_PROCESS_ID: &str = "pipeline-workbench-stdin-probe";
const ACTIVE_COMMAND_PROCESS_ID: &str = "pipeline-workbench-cleanup-probe";
const FIXTURE_CONTENT: &str = "pipeline-workbench-readable-fixture\n";
const STDIN_FIXTURE_CONTENT: &str = "pipeline-workbench-stdin-fixture\n";

const SECRET_ENVIRONMENT_KEYS: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "AZURE_OPENAI_API_KEY",
    "AZURE_OPENAI_ENDPOINT",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "CODEX_API_KEY",
];

#[derive(Debug, Clone, Serialize)]
pub struct QualificationProbeReport {
    pub schema_version: u32,
    pub executable: String,
    pub cli: DetectedCodexVersion,
    pub stable_schema_sha256: &'static str,
    pub experimental_schema_sha256: &'static str,
    pub isolated_codex_home_verified: bool,
    pub ambient_account_absent: bool,
    pub model_catalog_items: usize,
    pub permission_profiles: Vec<QualificationPermissionProfile>,
    pub active_permission_profile: Option<String>,
    pub experimental_thread_lifecycle_verified: bool,
    pub dynamic_tool_declaration_accepted: bool,
    pub read_only_sandbox_verified: bool,
    pub discovery_permissions_verified: bool,
    pub runtime_workspace_roots_verified: bool,
    pub instruction_sources_empty: bool,
    pub allowed_fixture_read_verified: bool,
    pub workbench_state_read_denied: bool,
    pub sibling_project_read_denied: bool,
    pub workspace_write_denied: bool,
    pub command_exec_stdin_verified: bool,
    pub command_exec_resize_verified: bool,
    pub command_exec_terminate_verified: bool,
    pub graceful_stdio_shutdown_verified: bool,
    pub active_command_connection_cleanup_verified: bool,
    pub platform_family: String,
    pub platform_os: String,
    pub user_agent: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QualificationPermissionProfile {
    pub id: String,
    pub allowed: bool,
}

#[derive(Debug, PartialEq)]
enum FrameKind<'a> {
    Response(&'a Value),
    Notification(&'a str),
    ServerRequest { id: &'a Value, method: &'a str },
}

fn classify_frame(value: &Value) -> Result<FrameKind<'_>, String> {
    let id = value.get("id");
    let method = value.get("method").and_then(Value::as_str);
    match (id, method) {
        (Some(id), Some(method)) => Ok(FrameKind::ServerRequest { id, method }),
        (None, Some(method)) => Ok(FrameKind::Notification(method)),
        (Some(_), None) if value.get("result").is_some() || value.get("error").is_some() => {
            Ok(FrameKind::Response(value))
        }
        _ => Err("Codex App Server emitted an unclassifiable frame".to_string()),
    }
}

async fn read_capped<R: AsyncRead + Unpin>(mut reader: R, limit: usize) -> (Vec<u8>, bool) {
    let mut bytes = Vec::with_capacity(limit.min(16 * 1024));
    let mut truncated = false;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let count = match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        let remaining = limit.saturating_sub(bytes.len());
        let kept = remaining.min(count);
        bytes.extend_from_slice(&chunk[..kept]);
        truncated |= kept < count;
    }
    (bytes, truncated)
}

pub(super) async fn installed_version(
    resolved: &crate::deps::ResolvedCommand,
) -> Result<DetectedCodexVersion, String> {
    let mut command = resolved.command(["--version"]);
    crate::pipeline::claude::configure_silent_command(&mut command);
    command
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = tokio::process::Command::from(command)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Failed to start Codex version probe: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Codex version probe has no stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Codex version probe has no stderr")?;
    let stdout_task = tokio::spawn(read_capped(stdout, VERSION_OUTPUT_LIMIT));
    let stderr_task = tokio::spawn(read_capped(stderr, VERSION_OUTPUT_LIMIT));
    let status = tokio::time::timeout(PROBE_TIMEOUT, child.wait())
        .await
        .map_err(|_| "Codex version probe timed out".to_string())?
        .map_err(|error| format!("Codex version probe failed: {error}"))?;
    let (stdout, stdout_truncated) = stdout_task
        .await
        .map_err(|error| format!("Codex version stdout reader failed: {error}"))?;
    let (stderr, _) = stderr_task
        .await
        .map_err(|error| format!("Codex version stderr reader failed: {error}"))?;
    if !status.success() {
        return Err(format!(
            "Codex version probe exited unsuccessfully: {}",
            String::from_utf8_lossy(&stderr).trim()
        ));
    }
    if stdout_truncated {
        return Err("Codex version response exceeded its safety limit".to_string());
    }
    detected_version(&String::from_utf8_lossy(&stdout))
}

async fn send(writer: &mut tokio::process::ChildStdin, message: &Value) -> Result<(), String> {
    let mut encoded = serde_json::to_vec(message)
        .map_err(|error| format!("Failed to encode Workbench probe request: {error}"))?;
    encoded.push(b'\n');
    writer
        .write_all(&encoded)
        .await
        .map_err(|error| format!("Failed to write Workbench probe request: {error}"))?;
    writer
        .flush()
        .await
        .map_err(|error| format!("Failed to flush Workbench probe request: {error}"))
}

async fn response_for(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    request_id: &Value,
) -> Result<Value, String> {
    for _ in 0..MAX_FRAMES_PER_RESPONSE {
        let Some(record) = crate::pipeline::logging::next_bounded_line(reader, FRAME_LIMIT)
            .await
            .map_err(|error| format!("Failed to read Workbench probe response: {error}"))?
        else {
            return Err("Codex App Server ended before replying".to_string());
        };
        if record.truncated {
            return Err(format!(
                "Codex App Server emitted a frame larger than {} MiB",
                FRAME_LIMIT / 1024 / 1024
            ));
        }
        let value: Value = serde_json::from_str(&record.text)
            .map_err(|error| format!("Codex App Server emitted invalid JSON: {error}"))?;
        match classify_frame(&value)? {
            FrameKind::Response(response) if response.get("id") == Some(request_id) => {
                return response_result(response);
            }
            FrameKind::Response(_) | FrameKind::Notification(_) => {}
            FrameKind::ServerRequest { method, .. } => {
                return Err(format!(
                    "Codex App Server sent unexpected request {method:?} during the no-turn probe"
                ));
            }
        }
    }
    Err("Codex App Server emitted too many unrelated frames".to_string())
}

fn response_result(response: &Value) -> Result<Value, String> {
    if let Some(error) = response.get("error") {
        return Err(format!("Codex App Server request failed: {error}"));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "Codex App Server response has no result".to_string())
}

async fn response_pair(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    first_id: &Value,
    second_id: &Value,
) -> Result<(Value, Value), String> {
    let mut first = None;
    let mut second = None;
    for _ in 0..MAX_FRAMES_PER_RESPONSE.saturating_mul(2) {
        let Some(record) = crate::pipeline::logging::next_bounded_line(reader, FRAME_LIMIT)
            .await
            .map_err(|error| format!("Failed to read Workbench probe response: {error}"))?
        else {
            return Err("Codex App Server ended before replying".to_string());
        };
        if record.truncated {
            return Err(format!(
                "Codex App Server emitted a frame larger than {} MiB",
                FRAME_LIMIT / 1024 / 1024
            ));
        }
        let value: Value = serde_json::from_str(&record.text)
            .map_err(|error| format!("Codex App Server emitted invalid JSON: {error}"))?;
        match classify_frame(&value)? {
            FrameKind::Response(response) if response.get("id") == Some(first_id) => {
                if first.is_some() {
                    return Err("Codex App Server replied twice to one request".to_string());
                }
                first = Some(response_result(response)?);
            }
            FrameKind::Response(response) if response.get("id") == Some(second_id) => {
                if second.is_some() {
                    return Err("Codex App Server replied twice to one request".to_string());
                }
                second = Some(response_result(response)?);
            }
            FrameKind::Response(_) | FrameKind::Notification(_) => {}
            FrameKind::ServerRequest { method, .. } => {
                return Err(format!(
                    "Codex App Server sent unexpected request {method:?} during the no-turn probe"
                ));
            }
        }
        if first.is_some() && second.is_some() {
            return Ok((first.take().unwrap(), second.take().unwrap()));
        }
    }
    Err("Codex App Server emitted too many frames before both replies".to_string())
}

async fn streamed_command_pid(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    request_id: &Value,
    process_id: &str,
) -> Result<u32, String> {
    let mut stdout = Vec::new();
    for _ in 0..MAX_FRAMES_PER_RESPONSE {
        let Some(record) = crate::pipeline::logging::next_bounded_line(reader, FRAME_LIMIT)
            .await
            .map_err(|error| format!("Failed to read Workbench command stream: {error}"))?
        else {
            return Err("Codex App Server ended before streaming command output".to_string());
        };
        if record.truncated {
            return Err(format!(
                "Codex App Server emitted a frame larger than {} MiB",
                FRAME_LIMIT / 1024 / 1024
            ));
        }
        let value: Value = serde_json::from_str(&record.text)
            .map_err(|error| format!("Codex App Server emitted invalid JSON: {error}"))?;
        match classify_frame(&value)? {
            FrameKind::Notification("command/exec/outputDelta")
                if value["params"]["processId"].as_str() == Some(process_id)
                    && value["params"]["stream"].as_str() == Some("stdout") =>
            {
                let encoded = value["params"]["deltaBase64"]
                    .as_str()
                    .ok_or("Command output notification omitted deltaBase64")?;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|error| format!("Command output was not valid base64: {error}"))?;
                if decoded.len() > 64usize.saturating_sub(stdout.len()) {
                    return Err("Command readiness output exceeded 64 bytes".to_string());
                }
                stdout.extend_from_slice(&decoded);
                if stdout.contains(&b'\n') {
                    let line = String::from_utf8_lossy(&stdout);
                    return line
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| format!("Command emitted an invalid process id: {line:?}"));
                }
            }
            FrameKind::Response(response) if response.get("id") == Some(request_id) => {
                return Err(format!(
                    "Cleanup command exited before its connection closed: {response}"
                ));
            }
            FrameKind::Response(_) | FrameKind::Notification(_) => {}
            FrameKind::ServerRequest { method, .. } => {
                return Err(format!(
                    "Codex App Server sent unexpected request {method:?} during command cleanup probe"
                ));
            }
        }
    }
    Err("Codex App Server emitted too many frames before command readiness".to_string())
}

fn same_directory(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn initialize_request(id: Value) -> Value {
    json!({
        "method": "initialize",
        "id": id,
        "params": {
            "clientInfo": {
                "name": "pipeline_workbench_probe",
                "title": "Pipeline Workbench Probe",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": { "experimentalApi": true }
        }
    })
}

fn initialized_notification() -> Value {
    json!({ "method": "initialized", "params": {} })
}

fn permission_profile_list_request(id: Value, cwd: &Path) -> Value {
    json!({
        "method": "permissionProfile/list",
        "id": id,
        "params": { "cwd": cwd, "limit": 100 }
    })
}

fn command_exec_request(id: Value, command: Vec<String>, cwd: &Path) -> Value {
    json!({
        "method": "command/exec",
        "id": id,
        "params": {
            "command": command,
            "cwd": cwd,
            "outputBytesCap": 4096,
            "permissionProfile": PROBE_PERMISSION_PROFILE,
            "timeoutMs": 5000
        }
    })
}

fn streaming_command_exec_request(
    id: Value,
    command: Vec<String>,
    cwd: &Path,
    process_id: &str,
) -> Value {
    json!({
        "method": "command/exec",
        "id": id,
        "params": {
            "command": command,
            "cwd": cwd,
            "outputBytesCap": 4096,
            "permissionProfile": PROBE_PERMISSION_PROFILE,
            "processId": process_id,
            "streamStdoutStderr": true,
            "timeoutMs": 30000
        }
    })
}

fn tty_command_exec_request(
    id: Value,
    command: Vec<String>,
    cwd: &Path,
    process_id: &str,
) -> Value {
    json!({
        "method": "command/exec",
        "id": id,
        "params": {
            "command": command,
            "cwd": cwd,
            "outputBytesCap": 4096,
            "permissionProfile": PROBE_PERMISSION_PROFILE,
            "processId": process_id,
            "size": { "rows": 24, "cols": 80 },
            "streamStdoutStderr": true,
            "timeoutMs": 30000,
            "tty": true
        }
    })
}

fn stdin_command_exec_request(id: Value, command: Vec<String>, cwd: &Path) -> Value {
    json!({
        "method": "command/exec",
        "id": id,
        "params": {
            "command": command,
            "cwd": cwd,
            "outputBytesCap": 4096,
            "permissionProfile": PROBE_PERMISSION_PROFILE,
            "processId": STDIN_COMMAND_PROCESS_ID,
            "streamStdin": true,
            "timeoutMs": 5000
        }
    })
}

fn command_exec_write_request(id: Value, process_id: &str, bytes: &[u8]) -> Value {
    json!({
        "method": "command/exec/write",
        "id": id,
        "params": {
            "processId": process_id,
            "deltaBase64": base64::engine::general_purpose::STANDARD.encode(bytes),
            "closeStdin": true
        }
    })
}

fn command_exec_resize_request(id: Value, process_id: &str) -> Value {
    json!({
        "method": "command/exec/resize",
        "id": id,
        "params": {
            "processId": process_id,
            "size": { "rows": 40, "cols": 120 }
        }
    })
}

fn command_exec_terminate_request(id: Value, process_id: &str) -> Value {
    json!({
        "method": "command/exec/terminate",
        "id": id,
        "params": { "processId": process_id }
    })
}

fn thread_start_request(id: Value, fixture_root: &Path) -> Value {
    json!({
        "method": "thread/start",
        "id": id,
        "params": {
            "approvalPolicy": "untrusted",
            "approvalsReviewer": "user",
            "cwd": fixture_root,
            "developerInstructions": "Workbench qualification fixture. Do not start work.",
            "dynamicTools": [{
                "type": "function",
                "name": "workbench_probe_echo",
                "description": "Return a harmless qualification fixture value.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "value": { "type": "string" } },
                    "required": ["value"],
                    "additionalProperties": false
                }
            }],
            // Persistence is confined to the temporary Codex home.
            // Ephemeral threads have no rollout for thread/read.
            "ephemeral": false,
            "modelProvider": "openai",
            "permissions": PROBE_PERMISSION_PROFILE,
            "runtimeWorkspaceRoots": [fixture_root],
        }
    })
}

#[cfg(unix)]
fn read_file_command(path: &Path) -> Vec<String> {
    vec!["/bin/cat".to_string(), path.display().to_string()]
}

#[cfg(unix)]
fn write_file_command(path: &Path) -> Vec<String> {
    vec!["/usr/bin/touch".to_string(), path.display().to_string()]
}

#[cfg(unix)]
fn long_running_command() -> Vec<String> {
    vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "printf '%s\\n' \"$$\"; exec /bin/sleep 30".to_string(),
    ]
}

#[cfg(unix)]
fn stdin_echo_command() -> Vec<String> {
    vec!["/bin/cat".to_string()]
}

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(unix)]
fn terminate_probe_process(pid: u32) {
    if pid > 0 && pid <= i32::MAX as u32 {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
fn active_command_cleanup_verified(pid: u32) -> Result<bool, String> {
    Ok(!process_is_running(pid))
}

#[cfg(not(unix))]
fn read_file_command(path: &Path) -> Vec<String> {
    vec![
        "cmd.exe".to_string(),
        "/D".to_string(),
        "/C".to_string(),
        "type".to_string(),
        path.display().to_string(),
    ]
}

#[cfg(not(unix))]
fn write_file_command(path: &Path) -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        "New-Item -ItemType File -LiteralPath $args[0]".to_string(),
        path.display().to_string(),
    ]
}

#[cfg(not(unix))]
fn long_running_command() -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        "Write-Output $PID; Start-Sleep -Seconds 30".to_string(),
    ]
}

#[cfg(not(unix))]
fn stdin_echo_command() -> Vec<String> {
    vec![
        "powershell.exe".to_string(),
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-Command".to_string(),
        "[Console]::Out.Write([Console]::In.ReadToEnd())".to_string(),
    ]
}

#[cfg(not(unix))]
fn active_command_cleanup_verified(_pid: u32) -> Result<bool, String> {
    Err("Active-command PID cleanup qualification is not implemented on this platform".to_string())
}

#[cfg(not(unix))]
fn terminate_probe_process(_pid: u32) {}

async fn wait_for_process_stopped(pid: u32) -> Result<bool, String> {
    for _ in 0..20 {
        if active_command_cleanup_verified(pid)? {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Ok(false)
}

/// Run the WB-00 no-login probe against an isolated, temporary `$CODEX_HOME`.
///
/// This sends no model turn. It verifies the handshake, the server-reported
/// runtime namespace, absence of inherited credentials, and model-catalog
/// response shape. It also exercises the candidate permissions profile with
/// standalone commands, but sends no authenticated model turn.
pub async fn run_qualification_probe() -> Result<QualificationProbeReport, String> {
    let resolved =
        crate::deps::resolve_command("codex").ok_or("No launchable Codex CLI was found on PATH")?;
    let executable = resolved.discovered_path().display().to_string();
    let cli = installed_version(&resolved).await?;
    if !cli.meets_minimum {
        return Err(format!(
            "Codex CLI {} is older than the minimum Workspace version {}",
            cli.version,
            super::compatibility::MINIMUM_CODEX_VERSION
        ));
    }

    let temporary = tempfile::Builder::new()
        .prefix("pipeline-workbench-probe-")
        .tempdir()
        .map_err(|error| format!("Failed to create Workbench probe directory: {error}"))?;
    let codex_home = temporary.path().join("codex");
    let fixture_root = temporary.path().join("fixture");
    let sibling_root = temporary.path().join("sibling-project");
    std::fs::create_dir(&codex_home)
        .and_then(|_| std::fs::create_dir(&fixture_root))
        .and_then(|_| std::fs::create_dir(&sibling_root))
        .map_err(|error| format!("Failed to prepare Workbench probe directories: {error}"))?;
    let fixture_file = fixture_root.join("readable.txt");
    let sibling_file = sibling_root.join("must-not-read.txt");
    std::fs::write(&fixture_file, FIXTURE_CONTENT)
        .map_err(|error| format!("Failed to write Workbench probe fixture: {error}"))?;
    std::fs::write(&sibling_file, "sibling project fixture\n")
        .map_err(|error| format!("Failed to write Workbench sibling fixture: {error}"))?;
    let launcher = resolved.canonical_program()?;
    write_probe_config(&codex_home, &fixture_root, &launcher)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&codex_home, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Failed to secure Workbench probe home: {error}"))?;
    }

    let args = ["app-server", "--listen", "stdio://", "--strict-config"];
    let mut command = resolved.canonical_command(args)?;
    crate::pipeline::claude::configure_silent_command(&mut command);
    command
        .current_dir(&fixture_root)
        .env("CODEX_HOME", &codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in SECRET_ENVIRONMENT_KEYS {
        command.env_remove(key);
    }
    let mut child = tokio::process::Command::from(command)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("Failed to start Codex App Server probe: {error}"))?;
    let mut stdin = child.stdin.take().ok_or("Codex App Server has no stdin")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Codex App Server has no stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Codex App Server has no stderr")?;
    let stderr_task = tokio::spawn(read_capped(stderr, STDERR_LIMIT));
    let mut reader = BufReader::new(stdout);

    let exchange = async {
        let initialize_id = json!(1);
        send(&mut stdin, &initialize_request(initialize_id.clone())).await?;
        let initialize = response_for(&mut reader, &initialize_id).await?;
        send(&mut stdin, &initialized_notification()).await?;

        let account_id = json!("account-read");
        send(
            &mut stdin,
            &json!({
                "method": "account/read",
                "id": account_id,
                "params": { "refreshToken": false }
            }),
        )
        .await?;
        let account = response_for(&mut reader, &account_id).await?;

        let models_id = json!(3);
        send(
            &mut stdin,
            &json!({
                "method": "model/list",
                "id": models_id,
                "params": { "limit": 100, "includeHidden": false }
            }),
        )
        .await?;
        let models = response_for(&mut reader, &models_id).await?;

        let permissions_id = json!("permission-profiles");
        send(
            &mut stdin,
            &permission_profile_list_request(permissions_id.clone(), &fixture_root),
        )
        .await?;
        let permissions = response_for(&mut reader, &permissions_id).await?;

        let thread_id = json!(4);
        send(
            &mut stdin,
            &thread_start_request(thread_id.clone(), &fixture_root),
        )
        .await?;
        let started = response_for(&mut reader, &thread_id).await?;
        let provider_thread_id = started["thread"]["id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or("Thread start response omitted its thread id")?
            .to_string();

        // `thread/start` does not materialize a rollout until history exists.
        // Inject one harmless user fixture so read/resume can be qualified
        // without starting a model turn or requiring authentication.
        let inject_id = json!(5);
        send(
            &mut stdin,
            &json!({
                "method": "thread/inject_items",
                "id": inject_id,
                "params": {
                    "threadId": provider_thread_id,
                    "items": [{
                        "type": "message",
                        "role": "user",
                        "content": [{
                            "type": "input_text",
                            "text": "Pipeline Workbench qualification fixture; no response requested."
                        }]
                    }]
                }
            }),
        )
        .await?;
        response_for(&mut reader, &inject_id).await?;

        let read_id = json!(6);
        send(
            &mut stdin,
            &json!({
                "method": "thread/read",
                "id": read_id,
                "params": { "threadId": provider_thread_id, "includeTurns": false }
            }),
        )
        .await?;
        let read = response_for(&mut reader, &read_id).await?;

        let resume_id = json!(7);
        send(
            &mut stdin,
            &json!({
                "method": "thread/resume",
                "id": resume_id,
                "params": {
                    "threadId": provider_thread_id,
                    "approvalPolicy": "untrusted",
                    "approvalsReviewer": "user",
                    "cwd": fixture_root,
                    "developerInstructions": "Workbench qualification fixture. Do not start work.",
                    "modelProvider": "openai",
                    "permissions": PROBE_PERMISSION_PROFILE,
                    "runtimeWorkspaceRoots": [fixture_root],
                }
            }),
        )
        .await?;
        let resumed = response_for(&mut reader, &resume_id).await?;

        let allowed_read_id = json!(8);
        send(
            &mut stdin,
            &command_exec_request(
                allowed_read_id.clone(),
                read_file_command(&fixture_file),
                &fixture_root,
            ),
        )
        .await?;
        let allowed_read = response_for(&mut reader, &allowed_read_id).await?;

        let denied_read_id = json!(9);
        send(
            &mut stdin,
            &command_exec_request(
                denied_read_id.clone(),
                read_file_command(&codex_home.join("config.toml")),
                &fixture_root,
            ),
        )
        .await?;
        let denied_read = response_for(&mut reader, &denied_read_id).await?;

        let denied_sibling_id = json!(10);
        send(
            &mut stdin,
            &command_exec_request(
                denied_sibling_id.clone(),
                read_file_command(&sibling_file),
                &fixture_root,
            ),
        )
        .await?;
        let denied_sibling = response_for(&mut reader, &denied_sibling_id).await?;

        let denied_write_path = fixture_root.join("write-must-fail");
        let denied_write_id = json!(11);
        send(
            &mut stdin,
            &command_exec_request(
                denied_write_id.clone(),
                write_file_command(&denied_write_path),
                &fixture_root,
            ),
        )
        .await?;
        let denied_write = response_for(&mut reader, &denied_write_id).await?;

        discovery::verify(
            &mut stdin,
            &mut reader,
            &fixture_root,
            &fixture_file,
            &codex_home.join("config.toml"),
            &sibling_file,
        )
        .await?;

        let terminated_command_id = json!(12);
        send(
            &mut stdin,
            &tty_command_exec_request(
                terminated_command_id.clone(),
                long_running_command(),
                &fixture_root,
                TERMINATED_COMMAND_PROCESS_ID,
            ),
        )
        .await?;
        let terminated_command_pid = streamed_command_pid(
            &mut reader,
            &terminated_command_id,
            TERMINATED_COMMAND_PROCESS_ID,
        )
        .await?;

        let resize_id = json!(13);
        send(
            &mut stdin,
            &command_exec_resize_request(resize_id.clone(), TERMINATED_COMMAND_PROCESS_ID),
        )
        .await?;
        let resize_response = response_for(&mut reader, &resize_id).await?;
        let command_exec_resize_verified = resize_response
            .as_object()
            .is_some_and(serde_json::Map::is_empty);
        if !command_exec_resize_verified {
            terminate_probe_process(terminated_command_pid);
            return Err(format!(
                "command/exec/resize returned an unexpected result: {resize_response}"
            ));
        }

        let terminate_id = json!(14);
        send(
            &mut stdin,
            &command_exec_terminate_request(terminate_id.clone(), TERMINATED_COMMAND_PROCESS_ID),
        )
        .await?;
        let (terminated_command, terminate_response) =
            response_pair(&mut reader, &terminated_command_id, &terminate_id).await?;
        let command_exec_terminate_verified = terminated_command["exitCode"]
            .as_i64()
            .is_some_and(|code| code != 0)
            && terminate_response
                .as_object()
                .is_some_and(serde_json::Map::is_empty)
            && wait_for_process_stopped(terminated_command_pid).await?;
        if !command_exec_terminate_verified {
            terminate_probe_process(terminated_command_pid);
            return Err(format!(
                "command/exec/terminate did not stop the fixture process: command={terminated_command}, terminate={terminate_response}"
            ));
        }

        let stdin_command_id = json!(15);
        send(
            &mut stdin,
            &stdin_command_exec_request(
                stdin_command_id.clone(),
                stdin_echo_command(),
                &fixture_root,
            ),
        )
        .await?;
        let stdin_write_id = json!(16);
        send(
            &mut stdin,
            &command_exec_write_request(
                stdin_write_id.clone(),
                STDIN_COMMAND_PROCESS_ID,
                STDIN_FIXTURE_CONTENT.as_bytes(),
            ),
        )
        .await?;
        let (stdin_command, stdin_write_response) =
            response_pair(&mut reader, &stdin_command_id, &stdin_write_id).await?;
        let command_exec_stdin_verified = stdin_command["exitCode"] == 0
            && stdin_command["stdout"].as_str() == Some(STDIN_FIXTURE_CONTENT)
            && stdin_command["stderr"]
                .as_str()
                .unwrap_or_default()
                .is_empty()
            && stdin_write_response
                .as_object()
                .is_some_and(serde_json::Map::is_empty);
        if !command_exec_stdin_verified {
            return Err(format!(
                "command/exec/write did not round-trip fixture stdin: command={stdin_command}, write={stdin_write_response}"
            ));
        }

        let active_command_id = json!(17);
        send(
            &mut stdin,
            &streaming_command_exec_request(
                active_command_id.clone(),
                long_running_command(),
                &fixture_root,
                ACTIVE_COMMAND_PROCESS_ID,
            ),
        )
        .await?;
        let active_command_pid =
            streamed_command_pid(&mut reader, &active_command_id, ACTIVE_COMMAND_PROCESS_ID)
                .await?;
        Ok::<_, String>((
            initialize,
            account,
            models,
            permissions,
            started,
            read,
            resumed,
            allowed_read,
            denied_read,
            denied_sibling,
            denied_write,
            denied_write_path,
            command_exec_stdin_verified,
            command_exec_resize_verified,
            command_exec_terminate_verified,
            active_command_pid,
        ))
    };

    let exchange_result = tokio::time::timeout(PROBE_TIMEOUT, exchange)
        .await
        .map_err(|_| "Codex App Server handshake probe timed out".to_string());
    drop(stdin);
    let graceful_exit = match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(status)) => Some(status),
        Ok(Err(error)) => {
            return Err(format!(
                "Failed while waiting for Codex App Server shutdown: {error}"
            ));
        }
        Err(_) => {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            None
        }
    };
    let (stderr, stderr_truncated) = stderr_task
        .await
        .map_err(|error| format!("Codex App Server stderr reader failed: {error}"))?;
    if stderr_truncated {
        return Err("Codex App Server diagnostic stderr exceeded 4 MiB".to_string());
    }
    let (
        initialize,
        account,
        models,
        permissions,
        started,
        read,
        resumed,
        allowed_read,
        denied_read,
        denied_sibling,
        denied_write,
        denied_write_path,
        command_exec_stdin_verified,
        command_exec_resize_verified,
        command_exec_terminate_verified,
        active_command_pid,
    ) = exchange_result.map_err(|error| {
        let diagnostic = String::from_utf8_lossy(&stderr);
        let diagnostic = diagnostic.lines().last().unwrap_or_default().trim();
        if diagnostic.is_empty() {
            error
        } else {
            format!("{error}. Last diagnostic: {diagnostic}")
        }
    })??;
    let graceful_stdio_shutdown_verified = graceful_exit.is_some_and(|status| status.success());
    if !graceful_stdio_shutdown_verified {
        return Err(
            "Codex App Server did not exit successfully after its stdio connection closed"
                .to_string(),
        );
    }
    let active_command_connection_cleanup_verified =
        wait_for_process_stopped(active_command_pid).await?;
    if !active_command_connection_cleanup_verified {
        terminate_probe_process(active_command_pid);
        return Err(format!(
            "Codex App Server left command process {active_command_pid} running after connection close"
        ));
    }

    let returned_home = initialize["codexHome"]
        .as_str()
        .ok_or("Initialize response omitted codexHome")?;
    let isolated_codex_home_verified = same_directory(Path::new(returned_home), &codex_home);
    if !isolated_codex_home_verified {
        return Err(format!(
            "Codex App Server did not use the isolated home (expected {}, received {returned_home})",
            codex_home.display()
        ));
    }
    let ambient_account_absent = account.get("account").is_some_and(Value::is_null);
    if !ambient_account_absent {
        return Err("The isolated Workbench probe unexpectedly inherited an account".to_string());
    }
    let model_catalog_items = models["data"]
        .as_array()
        .ok_or("Model list response omitted its data array")?
        .len();
    let permission_profiles = permissions["data"]
        .as_array()
        .ok_or("Permission profile response omitted its data array")?
        .iter()
        .map(|profile| {
            Ok(QualificationPermissionProfile {
                id: profile["id"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or("Permission profile omitted its id")?
                    .to_string(),
                allowed: profile["allowed"]
                    .as_bool()
                    .ok_or("Permission profile omitted its allowed state")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let active_permission_profile = started["activePermissionProfile"]["id"]
        .as_str()
        .map(str::to_string);
    let inspect_profile_available = permission_profiles
        .iter()
        .any(|profile| profile.id == PROBE_PERMISSION_PROFILE && profile.allowed);
    if !inspect_profile_available {
        return Err("The Workbench qualification permission profile is unavailable".to_string());
    }
    if active_permission_profile.as_deref() != Some(PROBE_PERMISSION_PROFILE) {
        return Err("Thread start did not activate the Workbench permission profile".to_string());
    }
    let started_thread_id = started["thread"]["id"]
        .as_str()
        .ok_or("Thread start response omitted its thread id")?;
    let read_thread_id = read["thread"]["id"]
        .as_str()
        .ok_or("Thread read response omitted its thread id")?;
    let resumed_thread_id = resumed["thread"]["id"]
        .as_str()
        .ok_or("Thread resume response omitted its thread id")?;
    let experimental_thread_lifecycle_verified = !started_thread_id.is_empty()
        && started_thread_id == read_thread_id
        && started_thread_id == resumed_thread_id;
    if !experimental_thread_lifecycle_verified {
        return Err("Thread start/read/resume did not preserve one thread identity".to_string());
    }
    let read_only_sandbox_verified =
        started["sandbox"]["type"] == "readOnly" && resumed["sandbox"]["type"] == "readOnly";
    if !read_only_sandbox_verified {
        return Err(
            "Thread lifecycle did not preserve the requested read-only sandbox".to_string(),
        );
    }
    let returned_roots = resumed["runtimeWorkspaceRoots"]
        .as_array()
        .ok_or("Thread resume response omitted runtimeWorkspaceRoots")?;
    let runtime_workspace_roots_verified = returned_roots.len() == 1
        && returned_roots[0]
            .as_str()
            .is_some_and(|root| same_directory(Path::new(root), &fixture_root));
    if !runtime_workspace_roots_verified {
        return Err("Thread lifecycle did not preserve the fixture workspace root".to_string());
    }
    let instruction_sources_empty = resumed["instructionSources"]
        .as_array()
        .is_some_and(Vec::is_empty);
    if !instruction_sources_empty {
        return Err("The isolated fixture unexpectedly loaded instruction sources".to_string());
    }
    let allowed_fixture_read_verified =
        allowed_read["exitCode"] == 0 && allowed_read["stdout"].as_str() == Some(FIXTURE_CONTENT);
    if !allowed_fixture_read_verified {
        return Err(format!(
            "The Workbench permission profile could not read its fixture root: {allowed_read}"
        ));
    }
    let workbench_state_read_denied = denied_read["exitCode"]
        .as_i64()
        .is_some_and(|code| code != 0)
        && denied_read["stdout"]
            .as_str()
            .unwrap_or_default()
            .is_empty();
    if !workbench_state_read_denied {
        return Err(format!(
            "The Workbench permission profile could read host-owned state: {denied_read}"
        ));
    }
    let sibling_project_read_denied = denied_sibling["exitCode"]
        .as_i64()
        .is_some_and(|code| code != 0)
        && denied_sibling["stdout"]
            .as_str()
            .unwrap_or_default()
            .is_empty();
    if !sibling_project_read_denied {
        return Err(format!(
            "The Workbench permission profile could read a sibling project: {denied_sibling}"
        ));
    }
    let workspace_write_denied = denied_write["exitCode"]
        .as_i64()
        .is_some_and(|code| code != 0)
        && !denied_write_path.exists();
    if !workspace_write_denied {
        return Err(format!(
            "The Workbench inspect profile allowed a fixture-root write: {denied_write}"
        ));
    }
    let platform_family = initialize["platformFamily"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or("Initialize response omitted platformFamily")?
        .to_string();
    let platform_os = initialize["platformOs"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or("Initialize response omitted platformOs")?
        .to_string();
    let user_agent = initialize["userAgent"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or("Initialize response omitted userAgent")?
        .to_string();

    Ok(QualificationProbeReport {
        schema_version: PROBE_SCHEMA_VERSION,
        executable,
        cli,
        stable_schema_sha256: STABLE_SCHEMA_SHA256,
        experimental_schema_sha256: EXPERIMENTAL_SCHEMA_SHA256,
        isolated_codex_home_verified,
        ambient_account_absent,
        model_catalog_items,
        permission_profiles,
        active_permission_profile,
        experimental_thread_lifecycle_verified,
        // A successful experimental thread/start response means the server
        // accepted the declaration shape. Actual call/response routing still
        // requires an authenticated opt-in model turn.
        dynamic_tool_declaration_accepted: true,
        read_only_sandbox_verified,
        discovery_permissions_verified: true,
        runtime_workspace_roots_verified,
        instruction_sources_empty,
        allowed_fixture_read_verified,
        workbench_state_read_denied,
        sibling_project_read_denied,
        workspace_write_denied,
        command_exec_stdin_verified,
        command_exec_resize_verified,
        command_exec_terminate_verified,
        graceful_stdio_shutdown_verified,
        active_command_connection_cleanup_verified,
        platform_family,
        platform_os,
        user_agent,
    })
}

#[cfg(test)]
mod tests;
