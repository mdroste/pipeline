//! Validate the effective App Server thread settings before admitting a turn.

use super::{required_string, same_path, string_array, RequestError, ThreadConnection};
use serde_json::Value;
use std::path::Path;

pub(super) fn parse_thread_connection(value: Value) -> Result<ThreadConnection, RequestError> {
    let thread_id = required_string(&value, &["thread", "id"], "thread response")?;
    let provider_session_id = required_string(&value, &["thread", "sessionId"], "thread response")?;
    let model = required_string(&value, &["model"], "thread response")?;
    let model_provider = required_string(&value, &["modelProvider"], "thread response")?;
    let cwd = required_string(&value, &["cwd"], "thread response")?;
    let instruction_sources = string_array(&value, "instructionSources")?;
    let runtime_workspace_roots = string_array(&value, "runtimeWorkspaceRoots")?;
    let active_permission_profile = value
        .get("activePermissionProfile")
        .and_then(|profile| profile.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let approval_policy = value
        .get("approvalPolicy")
        .and_then(Value::as_str)
        .map(str::to_string);
    let approvals_reviewer = value
        .get("approvalsReviewer")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(ThreadConnection {
        workbench_binding_id: None,
        thread_id,
        provider_session_id,
        model,
        model_provider,
        cwd,
        instruction_sources,
        runtime_workspace_roots,
        active_permission_profile,
        approval_policy,
        approvals_reviewer,
    })
}

pub(super) struct ApprovalSettings {
    pub policy: &'static str,
    pub reviewer: &'static str,
}

pub(super) fn approval_settings(
    permissions: &str,
    mode: crate::workbench::chat_approval::Mode,
) -> ApprovalSettings {
    use crate::workbench::chat_approval::Mode;
    if matches!(permissions, "discovery-inspect" | "discovery-edit") || mode == Mode::Sandbox {
        ApprovalSettings {
            policy: "never",
            reviewer: "user",
        }
    } else if mode == Mode::Auto {
        ApprovalSettings {
            policy: "on-request",
            reviewer: "auto_review",
        }
    } else {
        ApprovalSettings {
            policy: "untrusted",
            reviewer: "user",
        }
    }
}

pub(super) fn same_roots(connection: &ThreadConnection, expected_roots: &[String]) -> bool {
    connection.runtime_workspace_roots.len() == expected_roots.len()
        && expected_roots.iter().all(|expected| {
            connection
                .runtime_workspace_roots
                .iter()
                .any(|actual| same_path(Path::new(actual), Path::new(expected)))
        })
}

pub(super) fn validate_thread_contract(
    connection: &ThreadConnection,
    expected_cwd: Option<&str>,
    expected_roots: &[String],
    expected_permissions: &str,
    approval: ApprovalSettings,
    expected_model: Option<&str>,
) -> Result<(), RequestError> {
    if connection.model_provider != "openai" {
        return Err(RequestError::unavailable(
            "Codex App Server did not honor Workspace's OpenAI-only provider boundary",
        ));
    }
    let cwd_matches = expected_cwd
        .map(|expected| same_path(Path::new(&connection.cwd), Path::new(expected)))
        .unwrap_or_else(|| {
            expected_roots
                .iter()
                .any(|root| same_path(Path::new(&connection.cwd), Path::new(root)))
        });
    if !cwd_matches {
        return Err(RequestError::unavailable(
            "Codex App Server did not honor the requested Workspace working directory",
        ));
    }
    if !same_roots(connection, expected_roots) {
        return Err(RequestError::unavailable(
            "Codex App Server did not honor the requested Workspace filesystem roots",
        ));
    }
    if connection.active_permission_profile.as_deref() != Some(expected_permissions) {
        return Err(RequestError::unavailable(format!(
            "Codex App Server did not activate Workspace permission profile {expected_permissions}"
        )));
    }
    if connection.approval_policy.as_deref() != Some(approval.policy)
        || connection.approvals_reviewer.as_deref() != Some(approval.reviewer)
    {
        return Err(RequestError::unavailable(
            "Codex App Server did not preserve Workspace's approval policy",
        ));
    }
    if let Some(expected_model) = expected_model {
        if connection.model != expected_model {
            return Err(RequestError::unavailable(
                "Codex App Server substituted a different model despite fallback being disabled",
            ));
        }
    }
    Ok(())
}
