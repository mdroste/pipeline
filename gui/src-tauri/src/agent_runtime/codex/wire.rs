//! Exact 0.147.0 wire parsing and app-owned event normalization.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub user_agent: String,
    pub platform_family: String,
    pub platform_os: String,
    pub codex_home: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IncomingFrame {
    Response {
        id: u64,
        result: Option<Value>,
        error: Option<Value>,
    },
    Notification {
        method: String,
        params: Value,
    },
    ServerRequest {
        id: Value,
        method: String,
        params: Value,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NormalizedEvent {
    ConnectionReady {
        epoch: u64,
        initialize: InitializeResult,
    },
    ConnectionClosed {
        epoch: u64,
        reason: String,
        retryable: bool,
    },
    ThreadStarted {
        epoch: u64,
        thread_id: String,
        thread: Value,
    },
    ThreadStatusChanged {
        epoch: u64,
        thread_id: String,
        status: Value,
    },
    TurnStarted {
        epoch: u64,
        thread_id: String,
        turn_id: String,
        turn: Value,
    },
    TurnCompleted {
        epoch: u64,
        thread_id: String,
        turn_id: String,
        status: String,
        turn: Value,
    },
    ItemStarted {
        epoch: u64,
        thread_id: String,
        turn_id: String,
        item_id: String,
        item_kind: String,
        item: Value,
    },
    ItemCompleted {
        epoch: u64,
        thread_id: String,
        turn_id: String,
        item_id: String,
        item_kind: String,
        item: Value,
    },
    AgentMessageDelta {
        epoch: u64,
        thread_id: String,
        turn_id: String,
        item_id: String,
        delta: String,
    },
    AccountLoginCompleted {
        epoch: u64,
        login_id: Option<String>,
        success: bool,
        error: Option<String>,
    },
    AccountUpdated {
        epoch: u64,
        auth_mode: Option<String>,
        plan_type: Option<String>,
    },
    AccountRateLimitsUpdated {
        epoch: u64,
        rate_limits: Value,
    },
    ServerRequest {
        epoch: u64,
        request_id: Value,
        method: String,
        params: Value,
    },
    ServerRequestResolved {
        epoch: u64,
        thread_id: String,
        request_id: Value,
    },
    UnknownNotification {
        epoch: u64,
        method: String,
        params: Value,
    },
    ProtocolWarning {
        epoch: u64,
        message: String,
    },
}

pub(crate) fn parse_frame(value: Value) -> Result<IncomingFrame, String> {
    let id = value.get("id").cloned();
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string);
    match (id, method) {
        (Some(id), Some(method)) => Ok(IncomingFrame::ServerRequest {
            id,
            method,
            params: value.get("params").cloned().unwrap_or(Value::Null),
        }),
        (None, Some(method)) => Ok(IncomingFrame::Notification {
            method,
            params: value.get("params").cloned().unwrap_or(Value::Null),
        }),
        (Some(id), None) => {
            let id = id
                .as_u64()
                .ok_or("App Server response id was not an unsigned integer")?;
            let result = value.get("result").cloned();
            let error = value.get("error").cloned();
            if result.is_some() == error.is_some() {
                return Err(
                    "App Server response must contain exactly one of result or error".into(),
                );
            }
            Ok(IncomingFrame::Response { id, result, error })
        }
        (None, None) => Err("App Server emitted an unclassifiable frame".into()),
    }
}

fn object<'a>(
    params: &'a Value,
    context: &str,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    params
        .as_object()
        .ok_or_else(|| format!("{context} params were not an object"))
}

fn string(
    object: &serde_json::Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<String, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{context} omitted string field {key}"))
}

fn child<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<&'a Value, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{context} omitted field {key}"))
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
    context: &str,
) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{context} field {key} was not a string or null")),
    }
}

pub(crate) fn normalize_notification(
    epoch: u64,
    method: String,
    params: Value,
) -> Result<NormalizedEvent, String> {
    let context = method.as_str();
    match method.as_str() {
        "thread/started" => {
            let params_object = object(&params, context)?;
            let thread = child(params_object, "thread", context)?.clone();
            let thread_id = string(object(&thread, context)?, "id", context)?;
            Ok(NormalizedEvent::ThreadStarted {
                epoch,
                thread_id,
                thread,
            })
        }
        "thread/status/changed" => {
            let params_object = object(&params, context)?;
            Ok(NormalizedEvent::ThreadStatusChanged {
                epoch,
                thread_id: string(params_object, "threadId", context)?,
                status: child(params_object, "status", context)?.clone(),
            })
        }
        "turn/started" => normalize_turn(epoch, params, false),
        "turn/completed" => normalize_turn(epoch, params, true),
        "item/started" => normalize_item(epoch, params, false),
        "item/completed" => normalize_item(epoch, params, true),
        "item/agentMessage/delta" => {
            let params_object = object(&params, context)?;
            Ok(NormalizedEvent::AgentMessageDelta {
                epoch,
                thread_id: string(params_object, "threadId", context)?,
                turn_id: string(params_object, "turnId", context)?,
                item_id: string(params_object, "itemId", context)?,
                delta: string(params_object, "delta", context)?,
            })
        }
        "account/login/completed" => {
            let params_object = object(&params, context)?;
            Ok(NormalizedEvent::AccountLoginCompleted {
                epoch,
                login_id: optional_string(params_object, "loginId", context)?,
                success: params_object
                    .get("success")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| format!("{context} omitted boolean field success"))?,
                error: optional_string(params_object, "error", context)?,
            })
        }
        "account/updated" => {
            let params_object = object(&params, context)?;
            Ok(NormalizedEvent::AccountUpdated {
                epoch,
                auth_mode: optional_string(params_object, "authMode", context)?,
                plan_type: optional_string(params_object, "planType", context)?,
            })
        }
        "account/rateLimits/updated" => {
            let params_object = object(&params, context)?;
            Ok(NormalizedEvent::AccountRateLimitsUpdated {
                epoch,
                rate_limits: child(params_object, "rateLimits", context)?.clone(),
            })
        }
        "serverRequest/resolved" => {
            let params_object = object(&params, context)?;
            Ok(NormalizedEvent::ServerRequestResolved {
                epoch,
                thread_id: string(params_object, "threadId", context)?,
                request_id: child(params_object, "requestId", context)?.clone(),
            })
        }
        _ => Ok(NormalizedEvent::UnknownNotification {
            epoch,
            method,
            params,
        }),
    }
}

fn normalize_turn(epoch: u64, params: Value, completed: bool) -> Result<NormalizedEvent, String> {
    let context = if completed {
        "turn/completed"
    } else {
        "turn/started"
    };
    let params_object = object(&params, context)?;
    let thread_id = string(params_object, "threadId", context)?;
    let turn = child(params_object, "turn", context)?.clone();
    let turn_object = object(&turn, context)?;
    let turn_id = string(turn_object, "id", context)?;
    if completed {
        let status = string(turn_object, "status", context)?;
        Ok(NormalizedEvent::TurnCompleted {
            epoch,
            thread_id,
            turn_id,
            status,
            turn,
        })
    } else {
        Ok(NormalizedEvent::TurnStarted {
            epoch,
            thread_id,
            turn_id,
            turn,
        })
    }
}

fn normalize_item(epoch: u64, params: Value, completed: bool) -> Result<NormalizedEvent, String> {
    let context = if completed {
        "item/completed"
    } else {
        "item/started"
    };
    let params_object = object(&params, context)?;
    let thread_id = string(params_object, "threadId", context)?;
    let turn_id = string(params_object, "turnId", context)?;
    let item = child(params_object, "item", context)?.clone();
    let item_object = object(&item, context)?;
    let item_id = string(item_object, "id", context)?;
    let item_kind = string(item_object, "type", context)?;
    if completed {
        Ok(NormalizedEvent::ItemCompleted {
            epoch,
            thread_id,
            turn_id,
            item_id,
            item_kind,
            item,
        })
    } else {
        Ok(NormalizedEvent::ItemStarted {
            epoch,
            thread_id,
            turn_id,
            item_id,
            item_kind,
            item,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn response_and_server_request_ids_remain_distinct() {
        assert!(matches!(
            parse_frame(json!({"id": 4, "result": {}})).unwrap(),
            IncomingFrame::Response { id: 4, .. }
        ));
        assert!(matches!(
            parse_frame(json!({"id": "question-4", "method": "item/tool/requestUserInput", "params": {}})).unwrap(),
            IncomingFrame::ServerRequest { id, .. } if id == "question-4"
        ));
    }

    #[test]
    fn known_notifications_fail_closed_when_required_identity_is_missing() {
        assert!(normalize_notification(
            1,
            "turn/completed".to_string(),
            json!({"threadId": "thread-1", "turn": {"status": "completed"}}),
        )
        .is_err());
    }

    #[test]
    fn unknown_notifications_are_preserved_for_forward_compatibility() {
        let event = normalize_notification(
            7,
            "future/notification".to_string(),
            json!({"opaque": true}),
        )
        .unwrap();
        assert!(matches!(
            event,
            NormalizedEvent::UnknownNotification { epoch: 7, method, .. }
                if method == "future/notification"
        ));
    }

    #[test]
    fn account_notifications_are_normalized_without_losing_login_identity() {
        let completed = normalize_notification(
            11,
            "account/login/completed".to_string(),
            json!({"loginId":"login-7","success":false,"error":"denied"}),
        )
        .unwrap();
        assert!(matches!(
            completed,
            NormalizedEvent::AccountLoginCompleted {
                epoch: 11,
                login_id: Some(login_id),
                success: false,
                error: Some(error)
            } if login_id == "login-7" && error == "denied"
        ));

        let updated = normalize_notification(
            11,
            "account/updated".to_string(),
            json!({"authMode":"chatgpt","planType":"plus"}),
        )
        .unwrap();
        assert!(matches!(
            updated,
            NormalizedEvent::AccountUpdated {
                auth_mode: Some(auth_mode),
                plan_type: Some(plan_type),
                ..
            } if auth_mode == "chatgpt" && plan_type == "plus"
        ));
    }
}
