//! One pending sandboxed Bash approval per Workspace turn. Dropping the
//! waiting future, including on Stop or timeout, withdraws the request.

use super::*;
use crate::workbench::codex::RequestError;
use tokio::sync::oneshot;

pub(crate) const METHOD: &str = "claude/bash/requestApproval";

struct Pending {
    epoch: u64,
    id: Value,
    event: NormalizedEvent,
    decision: oneshot::Sender<bool>,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

pub(crate) fn pending() -> Option<NormalizedEvent> {
    PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|pending| pending.event.clone())
}

pub(crate) fn resolve(epoch: u64, id: &Value, result: Option<&Value>) -> Result<(), RequestError> {
    let accepted = match result.and_then(|value| value["decision"].as_str()) {
        Some("accept") => true,
        Some("decline") => false,
        _ => {
            return Err(RequestError::invalid(
                "Invalid Claude command approval decision",
            ))
        }
    };
    let mut slot = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    if !slot
        .as_ref()
        .is_some_and(|pending| pending.epoch == epoch && pending.id == *id)
    {
        return Err(RequestError::invalid(
            "Claude command approval is no longer pending",
        ));
    }
    slot.take()
        .unwrap()
        .decision
        .send(accepted)
        .map_err(|_| RequestError::invalid("Claude command approval is no longer pending"))
}

struct Waiting<'a, E: TurnEvents> {
    events: &'a E,
    turn: &'a Turn,
    id: Value,
}

impl<E: TurnEvents> Drop for Waiting<'_, E> {
    fn drop(&mut self) {
        let mut slot = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        if slot.as_ref().is_some_and(|pending| pending.id == self.id) {
            slot.take();
        }
        drop(slot);
        let _ = self
            .events
            .publish(&NormalizedEvent::ServerRequestResolved {
                epoch: self.turn.epoch,
                thread_id: self.turn.thread.clone(),
                request_id: self.id.clone(),
            });
    }
}

pub(super) async fn answer(
    frame: &Value,
    context: Option<(&Turn, &impl TurnEvents)>,
) -> Result<Value, String> {
    let deny = || json!({"behavior":"deny","message":"The sandboxed command was not approved."});
    let Some((turn, events)) = context else {
        return Ok(deny());
    };
    let Some(root) = &turn.approval_root else {
        return Ok(deny());
    };
    let request = &frame["request"];
    let input = &request["input"];
    let Some(command) = input["command"]
        .as_str()
        .filter(|cmd| !cmd.trim().is_empty() && cmd.len() <= 64 * 1024 && !cmd.contains('\0'))
    else {
        return Ok(deny());
    };
    if request["tool_name"] != "Bash"
        || !matches!(
            input.get("dangerouslyDisableSandbox"),
            None | Some(Value::Bool(false))
        )
    {
        return Ok(deny());
    }
    let id = json!(format!(
        "claude-approval-{}",
        protocol::identity().map_err(|e| e.message)?
    ));
    let event = NormalizedEvent::ServerRequest {
        epoch: turn.epoch,
        request_id: id.clone(),
        method: METHOD.into(),
        params: json!({
            "threadId": turn.thread,
            "turnId": turn.id,
            "command": command,
            "sandboxRoot": root,
            "inputKey": super::super::chat_approval::input_key(input),
            "reason": input["description"],
            "availableDecisions": ["accept", "decline"],
        }),
    };
    if super::super::runtime::has_saved_chat_approval(&event).await {
        return Ok(json!({"behavior":"allow","updatedInput":input}));
    }
    let (decision, wait) = oneshot::channel();
    {
        let mut slot = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return Err("A Claude command approval is already pending".into());
        }
        *slot = Some(Pending {
            epoch: turn.epoch,
            id: id.clone(),
            event: event.clone(),
            decision,
        });
    }
    let _waiting = Waiting { events, turn, id };
    events.publish_request(event)?;
    if wait.await.unwrap_or(false) {
        Ok(json!({"behavior":"allow","updatedInput":input}))
    } else {
        Ok(deny())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::turn_loop::RecordedEvents;

    #[tokio::test]
    async fn only_the_pending_sandboxed_command_can_be_allowed_once() {
        let turn = Turn {
            auth_mode: crate::agent_runtime::claude::AuthMode::Subscription,
            epoch: fresh_epoch(),
            thread: "thread".into(),
            id: "turn".into(),
            binding: "unused".into(),
            model: None,
            approval_root: Some(std::env::temp_dir().to_string_lossy().into_owned()),
            unsaved: Default::default(),
        };
        let events = RecordedEvents::default();
        let mut frame = json!({"request":{"tool_name":"Bash","input":{"command":"pwd","dangerouslyDisableSandbox":true}}});
        assert_eq!(
            answer(&frame, Some((&turn, &events))).await.unwrap()["behavior"],
            "deny"
        );
        frame["request"]["input"]["dangerouslyDisableSandbox"] = json!(false);
        for decision in [Some("accept"), Some("decline"), None] {
            let mut waiting = Box::pin(answer(&frame, Some((&turn, &events))));
            tokio::select! {
                biased;
                _ = &mut waiting => panic!("Bash returned before an approval decision"),
                _ = std::future::ready(()) => {}
            }
            let Some(NormalizedEvent::ServerRequest {
                request_id, params, ..
            }) = pending()
            else {
                panic!("Claude command was not pending");
            };
            assert_eq!(params["command"], "pwd");
            assert!(resolve(
                turn.epoch + 1,
                &request_id,
                Some(&json!({"decision":"accept"}))
            )
            .is_err());
            if let Some(decision) = decision {
                resolve(turn.epoch, &request_id, Some(&json!({"decision":decision}))).unwrap();
                let result = waiting.await.unwrap();
                assert_eq!(
                    result["behavior"],
                    if decision == "accept" {
                        "allow"
                    } else {
                        "deny"
                    }
                );
                if decision == "accept" {
                    assert_eq!(result["updatedInput"], frame["request"]["input"]);
                }
            } else {
                drop(waiting);
            }
            assert!(pending().is_none());
            assert!(resolve(turn.epoch, &request_id, Some(&json!({"decision":"accept"}))).is_err());
        }
    }
}
