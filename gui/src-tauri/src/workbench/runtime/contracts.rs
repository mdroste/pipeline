//! Application-owned turn contracts. Wire names remain stable.
use crate::workbench::store::WorkbenchError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCommandError {
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) retryable: bool,
    pub(crate) recovery: Option<String>,
}

impl From<WorkbenchError> for RuntimeCommandError {
    fn from(error: WorkbenchError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
            recovery: error.recovery,
        }
    }
}

impl From<crate::workbench::codex::RequestError> for RuntimeCommandError {
    fn from(error: crate::workbench::codex::RequestError) -> Self {
        Self {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
            recovery: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTurnRequest {
    pub(crate) session_id: String,
    pub(crate) text: String,
    pub(crate) client_submission_id: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
}

impl SendTurnRequest {
    // Explicit automation model choices do not inherit another model's effort.
    pub(crate) fn inherit_selection(&mut self, overrides: &Value) {
        if self.model.is_none() {
            self.model = overrides["model"].as_str().map(str::to_owned);
            if self.effort.is_none() {
                self.effort = overrides["effort"].as_str().map(str::to_owned);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTurnResult {
    pub(crate) epoch: u64,
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) draft_cleared: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveServerRequest {
    pub(crate) epoch: u64,
    pub(crate) request_id: Value,
    pub(crate) method: String,
    pub(crate) result: Option<Value>,
    pub(crate) decline_message: Option<String>,
}
