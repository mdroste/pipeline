//! Requests sent by the isolated qualification sequence.

use super::{PROBE_PERMISSION_PROFILE, STDIN_COMMAND_PROCESS_ID};
use base64::Engine as _;
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn initialize_request(id: Value) -> Value {
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

pub(super) fn initialized_notification() -> Value {
    json!({ "method": "initialized", "params": {} })
}

pub(super) fn permission_profile_list_request(id: Value, cwd: &Path) -> Value {
    json!({
        "method": "permissionProfile/list",
        "id": id,
        "params": { "cwd": cwd, "limit": 100 }
    })
}

pub(super) fn command_exec_request(id: Value, command: Vec<String>, cwd: &Path) -> Value {
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

pub(super) fn streaming_command_exec_request(
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

pub(super) fn tty_command_exec_request(
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

pub(super) fn stdin_command_exec_request(id: Value, command: Vec<String>, cwd: &Path) -> Value {
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

pub(super) fn command_exec_write_request(id: Value, process_id: &str, bytes: &[u8]) -> Value {
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

pub(super) fn command_exec_resize_request(id: Value, process_id: &str) -> Value {
    json!({
        "method": "command/exec/resize",
        "id": id,
        "params": {
            "processId": process_id,
            "size": { "rows": 40, "cols": 120 }
        }
    })
}

pub(super) fn command_exec_terminate_request(id: Value, process_id: &str) -> Value {
    json!({
        "method": "command/exec/terminate",
        "id": id,
        "params": { "processId": process_id }
    })
}

pub(super) fn thread_start_request(id: Value, fixture_root: &Path) -> Value {
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
