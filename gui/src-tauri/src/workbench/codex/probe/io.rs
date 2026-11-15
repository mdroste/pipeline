//! Bounded process output and correlated App Server frames.

use super::super::compatibility::{detected_version, DetectedCodexVersion};
use super::{FRAME_LIMIT, MAX_FRAMES_PER_RESPONSE, PROBE_TIMEOUT, VERSION_OUTPUT_LIMIT};
use base64::Engine as _;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, PartialEq)]
pub(super) enum FrameKind<'a> {
    Response(&'a Value),
    Notification(&'a str),
    ServerRequest { id: &'a Value, method: &'a str },
}

pub(super) fn classify_frame(value: &Value) -> Result<FrameKind<'_>, String> {
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

pub(super) async fn read_capped<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> (Vec<u8>, bool) {
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

pub(in crate::workbench::codex) async fn installed_version(
    resolved: &crate::deps::ResolvedCommand,
) -> Result<DetectedCodexVersion, String> {
    let mut command = resolved.command(["--version"]);
    crate::agent_runtime::process::configure_silent_command(&mut command);
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

pub(super) async fn send(
    writer: &mut tokio::process::ChildStdin,
    message: &Value,
) -> Result<(), String> {
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

pub(super) async fn response_for(
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

pub(super) async fn response_pair(
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

pub(super) async fn streamed_command_pid(
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
