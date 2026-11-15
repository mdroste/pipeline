//! Thin asynchronous Tauri facade for the blocking Workbench research store.

use super::store::{ReconcileResult, WorkbenchError, WorkbenchResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(test)]
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex as StdMutex;
use tauri::Emitter;
use tauri_plugin_shell::ShellExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCommandError {
    code: String,
    message: String,
    retryable: bool,
    recovery: Option<String>,
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

impl From<super::codex::RequestError> for RuntimeCommandError {
    fn from(error: super::codex::RequestError) -> Self {
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
    fn inherit_selection(&mut self, overrides: &Value) {
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
    epoch: u64,
    request_id: Value,
    method: String,
    result: Option<Value>,
    decline_message: Option<String>,
}

mod active_turn;
mod chat_approval;
pub(crate) mod codex;
mod desk;
mod project;
mod research;
mod sessions;
mod storage_gate;
mod tasks;
mod titles;
mod version_control;
mod workspaces;

#[cfg(test)]
#[path = "commands/tests.rs"]
mod tests;

#[cfg(test)]
use crate::agent_runtime::codex::is_safe_auth_url;
#[cfg(test)]
use active_turn::ActiveTurnState;
use codex::validate_pending_request;
#[cfg(test)]
use codex::validate_server_response;

use super::transcript_text::transcript_text;
pub use active_turn::*;
use active_turn::{
    active_turn_state, close_active_connection, complete_active_turn, turn_queue, TurnOrigin,
};
pub(crate) use chat_approval::has_saved_chat_approval;
use codex::ensure_event_bridge;
pub use codex::*;
pub use desk::*;
pub use project::*;
pub use research::*;
pub use version_control::*;

pub(crate) use sessions::clear_sent_draft;
pub use sessions::*;
#[cfg(test)]
use sessions::{clear_sent_draft_with, default_session_overrides};
use sessions::{conversation_turn_permit, ensure_session_idle};
use storage_gate::{run_storage_maintenance, run_store_exclusive};
pub(crate) use storage_gate::{run_store, run_store_owned};
#[cfg(test)]
use storage_gate::{storage_turn_permit, DATABASE_GATE, DATABASE_WORKER_LIMIT};
pub(crate) use tasks::{deliver_task, task_pending_request};
pub use titles::*;
pub(crate) use titles::{spawn_title_generation, TitleRuntime};
pub use workspaces::*;

pub(crate) use active_turn::research_team_permit;
