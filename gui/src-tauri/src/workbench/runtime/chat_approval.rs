//! Broker saved approvals only for live composer turns.

use super::codex::validate_pending_request;
use super::{active_turn_state, run_store, ResolveServerRequest, TurnOrigin};
use crate::agent_runtime::events::NormalizedEvent;
use crate::workbench::{chat_approval as grants, codex::RequestError};
use serde_json::json;

struct CommandGrant {
    session: String,
    thread: String,
    key: String,
}

fn command_grant(event: &NormalizedEvent) -> Option<CommandGrant> {
    let NormalizedEvent::ServerRequest {
        epoch,
        method,
        params,
        ..
    } = event
    else {
        return None;
    };
    let thread = params["threadId"].as_str()?;
    let state = active_turn_state()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let owner = state.owner.as_ref()?;
    if state.epoch != Some(*epoch)
        || state.thread_id.as_deref() != Some(thread)
        || state
            .turn_id
            .as_deref()
            .is_some_and(|turn| params["turnId"] != turn)
        || owner.origin != TurnOrigin::Composer
    {
        return None;
    }
    Some(CommandGrant {
        session: owner.session_id.clone(),
        thread: thread.into(),
        key: grants::command_key(method, params)?,
    })
}

pub(crate) fn can_remember(event: &NormalizedEvent) -> bool {
    command_grant(event).is_some()
}

pub(crate) async fn has_saved_chat_approval(event: &NormalizedEvent) -> bool {
    let Some(grant) = command_grant(event) else {
        return false;
    };
    run_store(move |store| grants::allowed(&store, &grant.session, &grant.thread, &grant.key))
        .await
        .unwrap_or(false)
}

async fn remember_chat_approval(event: &NormalizedEvent) -> Result<(), RequestError> {
    let grant = command_grant(event)
        .ok_or_else(|| RequestError::invalid("This request does not support Always allow"))?;
    run_store(move |store| grants::remember(&store, &grant.session, &grant.thread, &grant.key))
        .await
        .map_err(|error| RequestError::unavailable(error.message))
}

pub(crate) async fn remember_pending(
    request: &mut ResolveServerRequest,
) -> Result<(), RequestError> {
    if request
        .result
        .as_ref()
        .is_some_and(|result| result["decision"] == "alwaysAllow")
    {
        let event = if request.method == crate::workbench::claude::approval::METHOD {
            crate::workbench::claude::approval::pending()
        } else {
            super::codex::requests::pending_request(&request.request_id)
        };
        let event = event.filter(|event| matches!(event,
            NormalizedEvent::ServerRequest { request_id, .. } if request_id == &request.request_id
        )).ok_or_else(|| RequestError::invalid("Approval is no longer pending"))?;
        validate_pending_request(&event, request.epoch, &request.method)?;
        remember_chat_approval(&event).await?;
        request.result = Some(json!({"decision":"accept"}));
    }
    Ok(())
}
