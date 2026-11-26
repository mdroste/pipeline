//! Thin asynchronous Tauri facade for the blocking Workbench research store.

use super::store::{ReconcileResult, WorkbenchError, WorkbenchResult};

#[cfg(test)]
use serde_json::json;
use serde_json::Value;
#[cfg(test)]
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex as StdMutex;
use tauri_plugin_shell::ShellExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

mod active_turn;
pub(crate) mod codex;
mod desk;
mod project;
mod project_integrations;
mod research;
mod sessions;
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
use crate::workbench::runtime::admission::ActiveTurnState;
#[cfg(test)]
use crate::workbench::runtime::codex::validate_pending_request;
#[cfg(test)]
use crate::workbench::runtime::codex::validate_server_response;

use super::transcript_text::transcript_text;
pub use active_turn::*;
pub use codex::*;
pub use desk::*;
pub use project::*;
pub use project_integrations::*;
pub use research::*;
pub use version_control::*;

#[cfg(test)]
use sessions::default_session_overrides;
pub use sessions::*;
use sessions::{conversation_turn_permit, ensure_session_idle};
pub(crate) use tasks::deliver_task;
pub use titles::*;
pub use workspaces::*;

#[cfg(test)]
use crate::workbench::runtime::{
    accept_local_turn, clear_sent_draft, clear_sent_draft_with, complete_active_turn,
    finish_local_turn, run_store_owned, storage_turn_permit, ResolveServerRequest, DATABASE_GATE,
    DATABASE_WORKER_LIMIT,
};
use crate::workbench::runtime::{
    active_turn_state, run_storage_maintenance, run_store, run_store_exclusive, turn_queue,
    ActiveTurn, RuntimeCommandError, SendTurnRequest, SendTurnResult,
};

#[cfg(test)]
use crate::workbench::runtime::turn_has_live_owner;
