//! Conversation runtime services below the Tauri facade.
//! Admission owns permits; storage owns bounded workers; codex owns submission
//! and event reconciliation. Providers never import IPC wrappers.
pub(crate) mod admission;
pub(crate) mod chat_approval;
pub(crate) mod codex;
pub(crate) mod contracts;
pub(crate) mod session_state;
pub(crate) mod storage;
pub(crate) mod style_draft;
pub(crate) mod submission;
pub(crate) mod titles;
pub(crate) use admission::{
    accept_local_turn, active_turn_state, announce_active_turn, begin_turn_setup,
    close_active_connection, complete_active_turn, finish_local_turn, research_team_permit,
    stop_connection_tools, turn_has_live_owner, turn_queue, turn_setup_in_progress, with_session,
    ActiveTurn, PendingRequest, TurnOrigin,
};
pub(crate) use chat_approval::has_saved_chat_approval;
pub(crate) use codex::{
    codex_reconcile_session, ensure_event_bridge, interrupt_runtime_turn, reconcile_local_session,
    stop_codex_runtime,
};
pub(crate) use contracts::{
    ResolveServerRequest, RuntimeCommandError, SendTurnRequest, SendTurnResult,
};
pub(crate) use session_state::clear_sent_draft;
pub(crate) use storage::{
    run_storage_maintenance, run_store, run_store_exclusive, run_store_owned, run_store_phased,
};
pub(crate) use titles::{spawn_title_generation, TitleRuntime};

pub(crate) use submission::{submit_turn, task_turn_available};

pub(crate) mod finalization;

#[cfg(test)]
pub(crate) use session_state::clear_sent_draft_with;
#[cfg(test)]
pub(crate) use storage::{storage_turn_permit, DATABASE_GATE, DATABASE_WORKER_LIMIT};
