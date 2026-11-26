//! Codex server request validation and resolution.

use crate::agent_runtime::events::NormalizedEvent;
use crate::workbench::runtime::{
    chat_approval, with_session, PendingRequest, ResolveServerRequest,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static PENDING_SERVER_REQUESTS: OnceLock<Mutex<HashMap<String, NormalizedEvent>>> = OnceLock::new();

fn pending_server_requests() -> std::sync::MutexGuard<'static, HashMap<String, NormalizedEvent>> {
    PENDING_SERVER_REQUESTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub(crate) fn clear_pending() {
    pending_server_requests().clear();
}

pub(crate) fn record_pending(request_id: &Value, event: &NormalizedEvent) {
    pending_server_requests().insert(request_key(request_id), event.clone());
}

pub(crate) fn remove_pending(request_id: &Value) {
    pending_server_requests().remove(&request_key(request_id));
}

pub(crate) fn pending_request(request_id: &Value) -> Option<NormalizedEvent> {
    pending_server_requests()
        .get(&request_key(request_id))
        .cloned()
}

pub(crate) async fn codex_resolve_server_request(
    mut request: ResolveServerRequest,
) -> Result<(), crate::workbench::codex::RequestError> {
    chat_approval::remember_pending(&mut request).await?;
    if request.method == crate::workbench::claude::approval::METHOD {
        return crate::workbench::claude::approval::resolve(
            request.epoch,
            &request.request_id,
            request.result.as_ref(),
        );
    }
    let supervisor = crate::workbench::codex::supervisor_manager()
        .connect()
        .await?;
    let response = if let Some(result) = request.result.clone() {
        validate_server_response(&request.method, &result)?;
        Ok(result)
    } else {
        Err(crate::workbench::codex::RequestError::invalid(
            request
                .decline_message
                .clone()
                .unwrap_or_else(|| "The user declined this request".to_string()),
        ))
    };
    supervisor
        .respond_after_claim(request.request_id.clone(), response, || {
            claim_pending_request(
                &mut pending_server_requests(),
                &request,
                supervisor.status().epoch,
            )
        })
        .await?;
    Ok(())
}

pub(crate) fn claim_pending_request(
    pending: &mut HashMap<String, crate::agent_runtime::events::NormalizedEvent>,
    request: &ResolveServerRequest,
    epoch: u64,
) -> Result<(), crate::workbench::codex::RequestError> {
    if request.epoch != epoch {
        return Err(crate::workbench::codex::RequestError::invalid(
            "Server request belongs to an expired connection",
        ));
    }
    let key = request_key(&request.request_id);
    let event = pending.get(&key).ok_or_else(|| {
        crate::workbench::codex::RequestError::invalid("Server request is no longer pending")
    })?;
    validate_pending_request(event, request.epoch, &request.method)?;
    // Claim before the awaited write: an uncertain write must never be replayed.
    pending.remove(&key);
    Ok(())
}

pub(crate) fn codex_pending_requests() -> Vec<PendingRequest> {
    let mut pending = pending_server_requests()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pending.extend(crate::workbench::claude::approval::pending());
    pending.sort_by_key(|event| match event {
        crate::agent_runtime::events::NormalizedEvent::ServerRequest { request_id, .. } => {
            request_key(request_id)
        }
        _ => String::new(),
    });
    pending.into_iter().map(with_session).collect()
}

/// Refuses a server request no screen can answer. It would otherwise show a card
/// whose buttons can't reach Codex, and the turn would wait until Stop.
pub(crate) async fn refuse_unsupported_request(
    supervisor: &crate::workbench::codex::AppServerSupervisor,
    event: &crate::agent_runtime::events::NormalizedEvent,
) -> bool {
    let crate::agent_runtime::events::NormalizedEvent::ServerRequest {
        request_id, method, ..
    } = event
    else {
        return false;
    };
    let any_answer = json!({"decision":"decline","answers":{},"permissions":{}});
    if validate_server_response(method, &any_answer).is_ok() {
        return false;
    }
    let refusal = crate::workbench::codex::RequestError::invalid(format!(
        "Pipeline does not support {method}"
    ));
    let _ = supervisor
        .respond_to_server_request(request_id.clone(), Err(refusal))
        .await;
    true
}

fn request_key(request_id: &Value) -> String {
    serde_json::to_string(request_id).unwrap_or_else(|_| "invalid-request-id".to_string())
}

pub(crate) fn validate_pending_request(
    event: &crate::agent_runtime::events::NormalizedEvent,
    active_epoch: u64,
    submitted_method: &str,
) -> Result<(), crate::workbench::codex::RequestError> {
    match event {
        crate::agent_runtime::events::NormalizedEvent::ServerRequest { epoch, method, .. }
            if *epoch == active_epoch && method == submitted_method =>
        {
            Ok(())
        }
        crate::agent_runtime::events::NormalizedEvent::ServerRequest { epoch, .. }
            if *epoch != active_epoch =>
        {
            Err(crate::workbench::codex::RequestError::invalid(
                "Server request belongs to an expired connection",
            ))
        }
        crate::agent_runtime::events::NormalizedEvent::ServerRequest { .. } => Err(
            crate::workbench::codex::RequestError::invalid("Server request method did not match"),
        ),
        _ => Err(crate::workbench::codex::RequestError::invalid(
            "Pending request registry contained a non-request event",
        )),
    }
}

pub(crate) fn validate_server_response(
    method: &str,
    result: &Value,
) -> Result<(), crate::workbench::codex::RequestError> {
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            if !matches!(
                result.get("decision").and_then(Value::as_str),
                Some("accept" | "acceptForSession" | "decline" | "cancel")
            ) {
                return Err(crate::workbench::codex::RequestError::invalid(
                    "Invalid approval decision",
                ));
            }
        }
        "item/tool/requestUserInput" => {
            if !result.get("answers").is_some_and(Value::is_object) {
                return Err(crate::workbench::codex::RequestError::invalid(
                    "User-input response omitted answers",
                ));
            }
        }
        "item/permissions/requestApproval" => {
            if !result.get("permissions").is_some_and(Value::is_object) {
                return Err(crate::workbench::codex::RequestError::invalid(
                    "Permission response omitted permissions",
                ));
            }
        }
        _ => {
            return Err(crate::workbench::codex::RequestError::invalid(
                "Unsupported server request method",
            ))
        }
    }
    Ok(())
}

pub(crate) fn task_pending_request(thread: &str, turn: &str) -> Option<String> {
    pending_server_requests().values().find_map(|event| {
        if let crate::agent_runtime::events::NormalizedEvent::ServerRequest {
            method, params, ..
        } = event
        {
            if params.get("threadId").and_then(Value::as_str) == Some(thread)
                && params.get("turnId").and_then(Value::as_str) == Some(turn)
                && method != "item/tool/call"
            {
                return Some(
                    params
                        .get("reason")
                        .or_else(|| params.get("questions"))
                        .map(Value::to_string)
                        .unwrap_or_else(|| method.clone()),
                );
            }
        }
        None
    })
}
