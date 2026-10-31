//! App event records shared by conversation adapters. Native decoding stays
//! in the provider protocol module; serialized field names remain stable.
use super::codex::wire::InitializeResult;
use serde::Serialize;
use serde_json::Value;

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
