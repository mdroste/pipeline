//! The app's one running turn as the renderer sees it. The host owns this state,
//! so it outlives any page: the renderer reads `workbench_active_turn` when it
//! mounts and again after `activeTurnChanged`, and never polls. These records are
//! app-owned; no App Server shape reaches the renderer through them.

use super::{run_store, RuntimeCommandError};
pub(crate) fn research_team_permit() -> Result<OwnedSemaphorePermit, String> {
    turn_queue()
        .clone()
        .try_acquire_owned()
        .map_err(|_| "task_busy: Workspace is running another turn".into())
}

#[cfg(test)]
use super::{submit_turn, SendTurnRequest, DATABASE_GATE};
use crate::agent_runtime::events::NormalizedEvent;
#[cfg(test)]
use crate::workbench::store::UpdateSessionRequest;
use crate::workbench::{conversation_tools, research};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tauri::Emitter;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

static TURN_QUEUE: OnceLock<Arc<Semaphore>> = OnceLock::new();
static ACTIVE_TURN_STATE: OnceLock<StdMutex<ActiveTurnState>> = OnceLock::new();

pub(crate) fn turn_queue() -> &'static Arc<Semaphore> {
    TURN_QUEUE.get_or_init(|| Arc::new(Semaphore::new(1)))
}

#[derive(Default)]
pub(crate) struct ActiveTurnState {
    pub(crate) setup_in_progress: bool,
    pub(crate) epoch: Option<u64>,
    pub(crate) thread_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) completion_during_setup: Option<(String, String)>,
    pub(crate) connection_closed_during_setup: bool,
    pub(crate) permit: Option<OwnedSemaphorePermit>,
    pub(crate) account_lease: Option<tokio::sync::OwnedRwLockReadGuard<()>>,
    pub(crate) owner: Option<TurnOwner>,
}

impl ActiveTurnState {
    pub(crate) fn finish_setup(&mut self, failed: bool) {
        self.setup_in_progress = false;
        let completed = self
            .thread_id
            .as_ref()
            .zip(self.turn_id.as_ref())
            .is_some_and(|(thread, turn)| {
                self.completion_during_setup.as_ref() == Some(&(thread.clone(), turn.clone()))
            });
        if (failed && self.turn_id.is_none()) || self.connection_closed_during_setup || completed {
            *self = Self::default();
        }
    }
}

pub(crate) fn active_turn_state() -> &'static StdMutex<ActiveTurnState> {
    ACTIVE_TURN_STATE.get_or_init(|| StdMutex::new(ActiveTurnState::default()))
}

pub(crate) fn complete_active_turn(epoch: u64, thread_id: &str, turn_id: &str) {
    let mut state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.epoch != Some(epoch) || state.thread_id.as_deref() != Some(thread_id) {
        return;
    }
    if state.setup_in_progress {
        state.completion_during_setup = Some((thread_id.to_string(), turn_id.to_string()));
    } else if state.turn_id.as_deref() == Some(turn_id) {
        let thread_id = thread_id.to_owned();
        let turn_id = turn_id.to_owned();
        if research::jobs::has_active() {
            tokio::spawn(async move {
                let _ = run_store(move |store| {
                    research::cancel_executions_for_provider_turn(&store, &thread_id, &turn_id)
                })
                .await;
            });
        }
        *state = ActiveTurnState::default();
    }
}

pub(crate) fn accept_local_turn(epoch: u64, thread: &str, turn: &str) {
    let mut state = active_turn_state()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    state.epoch = Some(epoch);
    state.thread_id = Some(thread.into());
    state.turn_id = Some(turn.into());
}

pub(crate) fn finish_local_turn(epoch: u64, thread: &str, turn: &str) {
    complete_active_turn(epoch, thread, turn);
}

pub(crate) fn turn_setup_in_progress() -> bool {
    active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .setup_in_progress
}

/// Whether this native turn still holds the app-wide turn permit. Nothing does
/// after a restart, so a turn a crash left open has no live owner.
pub(crate) fn turn_has_live_owner(thread: &str, turn: &str) -> bool {
    let state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.thread_id.as_deref() == Some(thread) && state.turn_id.as_deref() == Some(turn)
}

/// Stops the tools a closed App Server connection leaves behind.
/// Only one turn runs at a time, so they are its turn's, or left from a finished
/// one, unless a Claude, API or newer ChatGPT turn holds the permit. That turn's
/// tools belong to it.
pub(crate) async fn stop_connection_tools(epoch: u64) {
    let another_runtime = {
        let state = active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.epoch != Some(epoch) && (state.setup_in_progress || state.epoch.is_some())
    };
    if !another_runtime {
        conversation_tools::stop_all().await;
    }
}

pub(crate) fn close_active_connection(epoch: u64) {
    let mut state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if state.epoch != Some(epoch) {
        return;
    }
    if let (Some(thread), Some(turn)) = (&state.thread_id, &state.turn_id) {
        let thread = thread.clone();
        let turn = turn.clone();
        if research::jobs::has_active() {
            tokio::spawn(async move {
                let _ = run_store(move |store| {
                    research::cancel_executions_for_provider_turn(&store, &thread, &turn)
                })
                .await;
            });
        }
    }
    if state.setup_in_progress {
        state.connection_closed_during_setup = true;
    } else {
        *state = ActiveTurnState::default();
    }
}

/// What put a turn under the permit, which decides who owns its text and Stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TurnOrigin {
    /// The composer: its text stays the session draft until acknowledged.
    Composer,
    /// A queued follow-up: no draft, and its conversation stops it.
    FollowUp,
    /// An Automation: no draft, and the Automations page stops it.
    Automation,
}

impl TurnOrigin {
    /// Only the coordinator passes a stop signal; a queued follow-up has none.
    pub(crate) fn of(task_bound: bool, coordinated: bool) -> Self {
        match (task_bound, coordinated) {
            (false, _) => Self::Composer,
            (true, false) => Self::FollowUp,
            (true, true) => Self::Automation,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TurnOwner {
    pub session_id: String,
    pub origin: TurnOrigin,
}

/// Takes the turn state for a submission that has just acquired the permit.
pub(crate) fn begin_turn_setup(
    session_id: String,
    origin: TurnOrigin,
    permit: OwnedSemaphorePermit,
) {
    let mut state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    state.setup_in_progress = true;
    state.epoch = None;
    state.thread_id = None;
    state.turn_id = None;
    state.completion_during_setup = None;
    state.connection_closed_during_setup = false;
    state.permit = Some(permit);
    state.owner = Some(TurnOwner { session_id, origin });
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTurn {
    session_id: String,
    title: Option<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    epoch: Option<u64>,
    /// Still being prepared. With no `turn_id`, the runtime has not acknowledged it.
    setup: bool,
    runtime: String,
    /// Belongs to an Automation, which owns stopping it.
    automated: bool,
    /// Sent from the composer, so its text is the session draft until acknowledged.
    composer: bool,
}

fn current_turn() -> Option<ActiveTurn> {
    let state = active_turn_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let owner = state.owner.as_ref()?;
    Some(ActiveTurn {
        session_id: owner.session_id.clone(),
        title: None,
        thread_id: state.thread_id.clone(),
        turn_id: state.turn_id.clone(),
        epoch: state.epoch,
        setup: state.setup_in_progress,
        runtime: "codex".into(),
        automated: owner.origin == TurnOrigin::Automation,
        composer: owner.origin == TurnOrigin::Composer,
    })
}

/// The running turn, or `None` when nothing holds the permit for a conversation.
/// A failed record read must not hide a turn that is running, so the conversation
/// details are best effort.
pub(crate) async fn active_turn() -> Option<ActiveTurn> {
    let mut turn = current_turn()?;
    let session_id = turn.session_id.clone();
    if let Ok(session) =
        run_store(move |store| Ok(store.session_snapshot(&session_id)?.session)).await
    {
        turn.title = Some(session.title);
        if let Some(runtime) = session.overrides["runtime"].as_str() {
            turn.runtime = runtime.into();
        }
    }
    Some(turn)
}

/// Tells every listener to reread the running turn. The event names no turn, so
/// the query stays the one source of identity. A failed submission is named,
/// because a page reopened during its setup has no promise of its own to await.
pub(crate) fn announce_active_turn(
    app: &tauri::AppHandle,
    failure: Option<(&str, &RuntimeCommandError)>,
) {
    let (session_id, send_error) = failure.unzip();
    let _ = app.emit(
        "workbench:event",
        json!({
            "kind": "activeTurnChanged",
            "epoch": 0,
            "sessionId": session_id,
            "sendError": send_error
        }),
    );
}

/// A server request and the conversation waiting on it. Only the turn holding the
/// permit can ask, and its thread is recorded before dispatch, so a request whose
/// thread is not that turn's has no conversation to name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRequest {
    #[serde(flatten)]
    event: NormalizedEvent,
    session_id: Option<String>,
    /// What the request card shows. Nothing in it is an App Server shape.
    details: Option<crate::workbench::codex::RequestDetails>,
    can_remember: bool,
}

fn request_session(state: &ActiveTurnState, event: &NormalizedEvent) -> Option<String> {
    let NormalizedEvent::ServerRequest { params, .. } = event else {
        return None;
    };
    let thread = params.get("threadId").and_then(Value::as_str)?;
    (state.thread_id.as_deref() == Some(thread))
        .then(|| state.owner.as_ref().map(|owner| owner.session_id.clone()))
        .flatten()
}

pub(crate) fn with_session(event: NormalizedEvent) -> PendingRequest {
    let session_id = request_session(
        &active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner()),
        &event,
    );
    let details = match &event {
        NormalizedEvent::ServerRequest { method, params, .. } => {
            crate::workbench::codex::request_details(method, params)
        }
        _ => None,
    };
    let can_remember = super::chat_approval::can_remember(&event);
    PendingRequest {
        event,
        session_id,
        details,
        can_remember,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::tests::{disposable_workspace, exclusive_turn_state};
    use crate::workbench::store::{CreateSessionRequest, Store};
    use std::time::Duration;

    fn conversation(store: &Store, title: &str, draft: &str) -> String {
        let created = store
            .create_session(CreateSessionRequest {
                workspace_id: None,
                title: title.into(),
                operation_id: format!("active-turn-{}", title.replace(' ', "-")),
            })
            .unwrap();
        store
            .update_session(UpdateSessionRequest {
                session_id: created.record.id.clone(),
                expected_revision: created.record.revision,
                operation_id: format!("active-turn-draft-{}", title.replace(' ', "-")),
                title: None,
                draft: Some(draft.into()),
                overrides: None,
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            })
            .unwrap();
        created.record.id
    }

    fn send(session_id: &str, submission: &str, text: &str) -> SendTurnRequest {
        SendTurnRequest {
            session_id: session_id.into(),
            text: text.into(),
            client_submission_id: submission.into(),
            model: None,
            effort: None,
        }
    }

    #[tokio::test]
    async fn the_active_turn_query_reports_the_owner_through_the_database_gate() {
        let _turn = exclusive_turn_state().await;
        let store = disposable_workspace();
        let session = conversation(&store, "Tariff paper", "");
        assert_eq!(active_turn().await, None);

        let permit = turn_queue().clone().try_acquire_owned().unwrap();
        begin_turn_setup(session.clone(), TurnOrigin::Composer, permit);
        let setup = serde_json::to_value(active_turn().await).unwrap();
        assert_eq!(
            setup,
            json!({
                "sessionId": session,
                "title": "Tariff paper",
                "threadId": null,
                "turnId": null,
                "epoch": null,
                "setup": true,
                "runtime": "codex",
                "automated": false,
                "composer": true
            })
        );

        // The query reads the conversation through the shared gate, so it waits
        // for whole-store work the same way every other store read does.
        let exclusive = DATABASE_GATE.write().await;
        let mut gated = Box::pin(active_turn());
        assert!(
            tokio::time::timeout(Duration::from_millis(50), gated.as_mut())
                .await
                .is_err()
        );
        drop(exclusive);
        assert_eq!(gated.await.unwrap().title.as_deref(), Some("Tariff paper"));

        accept_local_turn(7, "api-thread", "api-turn");
        let accepted = active_turn().await.unwrap();
        assert_eq!(accepted.thread_id.as_deref(), Some("api-thread"));
        assert_eq!(accepted.turn_id.as_deref(), Some("api-turn"));
        assert_eq!(accepted.epoch, Some(7));
        assert!(accepted.setup);

        active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .finish_setup(false);
        assert!(!active_turn().await.unwrap().setup);

        finish_local_turn(7, "api-thread", "api-turn");
        assert_eq!(active_turn().await, None);
        assert_eq!(turn_queue().available_permits(), 1);
    }

    #[tokio::test]
    async fn the_active_turn_names_a_runtime_and_automation_for_every_origin() {
        let _turn = exclusive_turn_state().await;
        let store = disposable_workspace();
        let session = conversation(&store, "Automated run", "");
        let record = store.session_snapshot(&session).unwrap().session;
        store
            .update_session(UpdateSessionRequest {
                session_id: session.clone(),
                expected_revision: record.revision,
                operation_id: "active-turn-runtime".into(),
                title: None,
                draft: None,
                overrides: Some(json!({"runtime": "claude"})),
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            })
            .unwrap();
        for (origin, automated, composer) in [
            (TurnOrigin::Automation, true, false),
            (TurnOrigin::FollowUp, false, false),
            (TurnOrigin::Composer, false, true),
        ] {
            let permit = turn_queue().clone().try_acquire_owned().unwrap();
            begin_turn_setup(session.clone(), origin, permit);
            let turn = active_turn().await.unwrap();
            assert_eq!(turn.runtime, "claude");
            assert_eq!((turn.automated, turn.composer), (automated, composer));
            active_turn_state()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .finish_setup(true);
            assert_eq!(turn_queue().available_permits(), 1);
        }
    }

    #[tokio::test]
    async fn a_send_is_refused_at_once_and_changes_nothing_while_the_permit_is_held() {
        let _turn = exclusive_turn_state().await;
        let store = disposable_workspace();
        let running = conversation(&store, "Running", "");
        let waiting = conversation(&store, "Waiting", "Earlier draft");
        let before = store.conversation_snapshot(&waiting).unwrap();

        let permit = turn_queue().clone().try_acquire_owned().unwrap();
        begin_turn_setup(running.clone(), TurnOrigin::Composer, permit);
        let refused = tokio::time::timeout(
            Duration::from_secs(2),
            Box::pin(submit_turn(
                None,
                send(&waiting, "submission-refused", "Second question"),
                None,
                Default::default(),
            )),
        )
        .await
        .expect("a held permit must refuse the send, not make it wait")
        .unwrap_err();
        let refused = serde_json::to_value(refused).unwrap();
        assert_eq!(refused["code"], "turn_busy");
        assert_eq!(refused["retryable"], true);

        // An automation's send is still reported as its own kind of busy.
        let (_stop, stopped) = tokio::sync::watch::channel(false);
        let busy_task = Box::pin(submit_turn(
            None,
            send(&waiting, "submission-task", "Scheduled question"),
            Some(crate::workbench::tasks::TaskBinding {
                session_id: waiting.clone(),
                workspace_id: None,
                cursor: String::new(),
                harness_fingerprint: String::new(),
                connection_id: None,
                runtime_root: String::new(),
                root_identity: String::new(),
                permission_profile: None,
                preset_id: None,
                access: None,
            }),
            crate::workbench::submission::Control(Some(stopped)),
        ))
        .await
        .unwrap_err();
        assert_eq!(
            serde_json::to_value(busy_task).unwrap()["code"],
            "task_busy"
        );

        let after = store.conversation_snapshot(&waiting).unwrap();
        assert_eq!(after.session.draft, "Earlier draft");
        assert_eq!(after.session.revision, before.session.revision);
        assert!(after.turns.is_empty());
        assert_eq!(
            active_turn().await.unwrap().session_id,
            running,
            "a refusal leaves the running turn's state alone"
        );

        active_turn_state()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .finish_setup(true);
        let unsent = serde_json::to_value(
            Box::pin(submit_turn(
                None,
                send(&waiting, "submission-admitted", "Second question"),
                None,
                Default::default(),
            ))
            .await
            .unwrap_err(),
        )
        .unwrap();
        assert_eq!(
            unsent["code"], "turn_not_sent",
            "once free, the same send reaches setup and stops only for want of a runtime"
        );
        assert_eq!(
            store.session_snapshot(&waiting).unwrap().session.draft,
            "Second question"
        );
        assert_eq!(active_turn().await, None);
    }

    #[tokio::test]
    async fn a_claude_send_that_fails_in_setup_is_reported_as_not_sent() {
        let _turn = exclusive_turn_state().await;
        let store = disposable_workspace();
        let session = conversation(&store, "Claude setup", "Saved question");
        let record = store.session_snapshot(&session).unwrap().session;
        store
            .update_session(UpdateSessionRequest {
                session_id: session.clone(),
                expected_revision: record.revision,
                operation_id: "claude-setup-runtime".into(),
                title: None,
                draft: None,
                overrides: Some(json!({"runtime": "claude"})),
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            })
            .unwrap();
        let failed = Box::pin(submit_turn(
            None,
            send(&session, "submission-claude-setup", "Saved question"),
            None,
            Default::default(),
        ))
        .await
        .unwrap_err();
        assert_eq!(
            serde_json::to_value(failed).unwrap()["code"],
            "turn_not_sent"
        );
        assert_eq!(active_turn().await, None);
    }

    #[test]
    fn a_turn_belongs_to_the_automation_page_only_when_the_coordinator_can_stop_it() {
        assert_eq!(TurnOrigin::of(false, false), TurnOrigin::Composer);
        assert_eq!(TurnOrigin::of(true, false), TurnOrigin::FollowUp);
        assert_eq!(TurnOrigin::of(true, true), TurnOrigin::Automation);
    }

    #[test]
    fn a_pending_request_names_the_conversation_only_for_the_running_thread() {
        let queue = Arc::new(Semaphore::new(1));
        let state = ActiveTurnState {
            thread_id: Some("thread-a".into()),
            permit: Some(queue.try_acquire_owned().unwrap()),
            owner: Some(TurnOwner {
                session_id: "session-a".into(),
                origin: TurnOrigin::Automation,
            }),
            ..Default::default()
        };
        let request = |thread: &str| NormalizedEvent::ServerRequest {
            epoch: 3,
            request_id: json!(5),
            method: "item/fileChange/requestApproval".into(),
            params: json!({"threadId": thread, "turnId": "turn"}),
        };
        let session = |event: NormalizedEvent| {
            let session_id = request_session(&state, &event);
            serde_json::to_value(PendingRequest {
                event,
                session_id,
                details: None,
                can_remember: false,
            })
            .unwrap()
        };
        let named = session(request("thread-a"));
        assert_eq!(named["sessionId"], "session-a");
        assert_eq!(named["kind"], "serverRequest");
        assert_eq!(named["requestId"], 5);
        assert_eq!(named["params"]["threadId"], "thread-a");
        assert_eq!(session(request("thread-b"))["sessionId"], Value::Null);
    }

    #[test]
    fn a_pending_request_reaches_the_card_as_an_app_record() {
        let event = NormalizedEvent::ServerRequest {
            epoch: 3,
            request_id: json!(5),
            method: "item/fileChange/requestApproval".into(),
            params: json!({"threadId": "t", "turnId": "u", "grantRoot": "/output"}),
        };
        let pending = serde_json::to_value(with_session(event)).unwrap();
        assert_eq!(pending["kind"], "serverRequest");
        assert_eq!(pending["details"]["type"], "fileChange");
        assert_eq!(pending["details"]["grantRoot"], "/output");
        assert_eq!(pending["details"]["declineDecision"], "decline");
    }
}
