//! Workspace turns as an automation sees them: how they end, and what Retry
//! takes up afterwards. These use a real conversation in the disposable store.
use super::coordinator::coordinator_fixture;
use super::*;
use crate::agent_runtime::events::NormalizedEvent;
use crate::orchestration::commands::control;
use crate::workbench::{api, commands, store as workspace, tasks};
use std::time::Duration;

/// A prepared automation with its Workspace step started and its turn accepted.
async fn started_step(
    w: &workspace::Store,
    m: &Coordinator,
    namespace: &str,
) -> (TaskRun, String, String) {
    let operation = store::id();
    let session = crate::workbench::discovery::source(w, &operation, "Turns", false).unwrap();
    let plan = chain(vec![step(
        "ask",
        Action::Workspace {
            prompt: "Inspect".into(),
            model: None,
            effort: None,
        },
    )]);
    let scope = adapters::prepare(&plan, Some(session.clone()), &json!({}))
        .await
        .unwrap();
    let mut run = m
        .store
        .create(&operation, plan, scope, json!({}), now(), true)
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    let receipt = run.progress.receipts.get_mut("ask").unwrap();
    receipt.state = "running".into();
    run.state = "running".into();
    let thread = format!("thread-{operation}");
    let binding = w.bind_session(&session, namespace, &thread).unwrap();
    w.record_turn_submission(
        &binding.id,
        &format!("task-{}", receipt.operation),
        "turn-1",
    )
    .unwrap();
    m.store.save(&mut run, "stepStarted", "Started").unwrap();
    (run, session, thread)
}

fn end_turn(w: &workspace::Store, session: &str, turn: &str, status: &str, error: Value) {
    let binding = w.active_binding(session).unwrap().unwrap();
    w.project_codex_event(
        &binding.id,
        &NormalizedEvent::TurnCompleted {
            epoch: 1,
            thread_id: binding.provider_thread_id.clone(),
            turn_id: turn.into(),
            status: status.into(),
            turn: json!({"id":turn,"status":status,"error":error}),
        },
    )
    .unwrap();
}

fn outcome(w: &workspace::Store, run: &TaskRun, session: &str) -> Value {
    let operation = &run.progress.receipts["ask"].operation;
    tasks::outcome(w, session, &format!("task-{operation}"))
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn a_failed_turn_leaves_the_automation_able_to_retry_and_says_why() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let (run, session, _) = started_step(&w, &m, api::NAMESPACE).await;
    end_turn(
        &w,
        &session,
        "turn-1",
        "failed",
        json!({"message":"Upstream said 429","codexErrorInfo":"usageLimitExceeded"}),
    );
    let stale = adapters::task_binding(&run.scope).unwrap();
    assert!(tasks::validate_binding(&w, &stale).is_err());

    m.complete(&run.id, "ask", Ok(outcome(&w, &run, &session)))
        .await
        .unwrap();
    let run = m.store.get(&run.id).unwrap();
    assert_eq!(run.state, "attention");
    assert_eq!(
        run.reason.as_deref(),
        Some("This step's conversation turn failed: The ChatGPT usage limit was reached. Retry after it resets.")
    );
    let current = adapters::task_binding(&run.scope).unwrap();
    tasks::validate_binding(&w, &current).unwrap();
}

#[tokio::test]
async fn retry_after_a_step_lost_its_result_takes_up_that_steps_own_turn() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let (run, session, _) = started_step(&w, &m, api::NAMESPACE).await;
    // The result was lost and the turn is still open, as after a crash.
    m.complete(&run.id, "ask", Err("Action worker stopped".into()))
        .await
        .unwrap();
    let run = m.store.get(&run.id).unwrap();
    assert_eq!(run.progress.receipts["ask"].state, "unknown");

    let retried = control(&m, run.id.clone(), run.revision, "retry".into())
        .await
        .unwrap();
    assert_eq!(retried.state, "queued");
    assert_eq!(outcome(&w, &run, &session)["state"], "interrupted");
    let current = adapters::task_binding(&retried.scope).unwrap();
    tasks::validate_binding(&w, &current).unwrap();
}

#[tokio::test]
async fn retry_never_takes_up_a_turn_the_automation_did_not_send() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let (run, session, _) = started_step(&w, &m, api::NAMESPACE).await;
    end_turn(&w, &session, "turn-1", "interrupted", Value::Null);
    let binding = w.active_binding(&session).unwrap().unwrap();
    w.record_turn_submission(&binding.id, "researcher-message", "turn-2")
        .unwrap();
    end_turn(&w, &session, "turn-2", "completed", Value::Null);
    m.complete(&run.id, "ask", Err("Action worker stopped".into()))
        .await
        .unwrap();
    let run = m.store.get(&run.id).unwrap();

    let retried = control(&m, run.id.clone(), run.revision, "retry".into())
        .await
        .unwrap();
    assert_eq!(retried.scope.session_cursor, run.scope.session_cursor);
    let stale = adapters::task_binding(&retried.scope).unwrap();
    assert!(tasks::validate_binding(&w, &stale).is_err());
}

#[tokio::test]
async fn check_saved_results_settles_a_plain_automations_lost_turn() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let (run, session, _) = started_step(&w, &m, api::NAMESPACE).await;
    m.complete(&run.id, "ask", Err("Action worker stopped".into()))
        .await
        .unwrap();
    let run = m.store.get(&run.id).unwrap();
    let reconciled = control(&m, run.id.clone(), run.revision, "reconcile".into())
        .await
        .unwrap();
    let receipt = &reconciled.progress.receipts["ask"];
    assert_eq!(receipt.state, "failed");
    assert_eq!(
        receipt.error.as_deref(),
        Some("This step's conversation turn was interrupted: The API connection ended; this turn was not replayed.")
    );
    assert_eq!(outcome(&w, &run, &session)["state"], "interrupted");
}

#[tokio::test]
async fn a_turn_ending_with_no_reason_says_so_in_plain_words() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    for (status, error, expected) in [
        (
            "failed",
            json!("Provider refused the request"),
            "This step's conversation turn failed: Provider refused the request",
        ),
        (
            "failed",
            json!({"message":"Sign in again","codexErrorInfo":"unauthorized"}),
            "This step's conversation turn failed: ChatGPT sign-in needs attention. Reconnect in Settings, then retry.",
        ),
        (
            "failed",
            json!({"message":"Stream dropped","codexErrorInfo":{"responseStreamDisconnected":{}}}),
            "This step's conversation turn failed: Stream dropped",
        ),
        (
            "interrupted",
            Value::Null,
            "This step's conversation turn was interrupted. No reason was recorded.",
        ),
    ] {
        let (run, session, _) = started_step(&w, &m, api::NAMESPACE).await;
        end_turn(&w, &session, "turn-1", status, error);
        let value = outcome(&w, &run, &session);
        assert_eq!(adapters::failure_message(&value), expected);
    }
}

const NO_GRACE: Duration = Duration::from_millis(1);

async fn wait_for_turn(
    run: &TaskRun,
    session: &str,
    thread: &str,
    grace: Duration,
) -> crate::orchestration::store::Result<Value> {
    let (_stop, mut receiver) = tokio::sync::watch::channel(false);
    let submission = format!("task-{}", run.progress.receipts["ask"].operation);
    adapters::await_turn(
        &None,
        session,
        &submission,
        thread,
        "turn-1",
        &mut receiver,
        grace,
    )
    .await
}

#[tokio::test]
async fn a_turn_whose_connection_ended_is_settled_instead_of_waiting_for_the_time_limit() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let (run, session, thread) = started_step(&w, &m, api::NAMESPACE).await;
    let ended = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_turn(&run, &session, &thread, NO_GRACE),
    )
    .await
    .expect("the action must not wait for its time limit")
    .unwrap();
    assert_eq!(ended["state"], "interrupted");
    assert_eq!(
        adapters::failure_message(&ended),
        "This step's conversation turn was interrupted: The API connection ended; this turn was not replayed."
    );
}

#[tokio::test]
async fn a_turn_no_runtime_can_settle_reports_the_lost_connection() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    // With no desktop handle, App Server can't be reached.
    let (run, session, thread) = started_step(&w, &m, "codex-app-server-v1").await;
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_turn(&run, &session, &thread, NO_GRACE),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.contains("connection ended"), "{error}");
    assert_eq!(outcome(&w, &run, &session)["state"], "running");
}

#[tokio::test]
async fn a_turn_with_a_live_owner_is_left_alone() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let (run, session, thread) = started_step(&w, &m, api::NAMESPACE).await;
    commands::accept_local_turn(1, &thread, "turn-1");
    let waited = tokio::time::timeout(
        Duration::from_millis(700),
        wait_for_turn(&run, &session, &thread, NO_GRACE),
    )
    .await;
    commands::finish_local_turn(1, &thread, "turn-1");
    assert!(waited.is_err(), "an owned turn is still running");
    assert_eq!(outcome(&w, &run, &session)["state"], "running");
}

/// A conversation in a project with a folder, and a check approved as it stands.
#[cfg(unix)]
fn approved_check(
    w: &workspace::Store,
    root: &std::path::Path,
    operation: &str,
) -> (String, String) {
    use crate::workbench::research;
    std::fs::write(root.join("analysis.py"), "print(1)\n").unwrap();
    let project = w
        .create_workspace(workspace::CreateWorkspaceRequest {
            name: "Checks".into(),
            root: Some(root.to_string_lossy().into()),
            operation_id: format!("workspace-{operation}"),
        })
        .unwrap()
        .record
        .id;
    let session = w
        .create_session(workspace::CreateSessionRequest {
            workspace_id: Some(project.clone()),
            title: "Check source".into(),
            operation_id: format!("session-{operation}"),
        })
        .unwrap()
        .record
        .id;
    let profile = research::save_execution_profile(
        w,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: project,
            name: "Analysis".into(),
            adapter: "command".into(),
            argv: vec!["/usr/bin/python3".into(), "analysis.py".into()],
            cwd: root.to_string_lossy().into(),
            environment: json!({}),
            inputs: vec!["analysis.py".into()],
            timeout_seconds: 30,
            outputs: Vec::new(),
            expected_revision: None,
            operation_id: format!("profile-{operation}"),
        },
    )
    .unwrap()
    .id;
    let fingerprint = research::preview_host_execution(w, &profile)
        .unwrap()
        .fingerprint;
    research::authorize_host_execution(w, &profile, &fingerprint).unwrap();
    (session, profile)
}

#[cfg(unix)]
#[tokio::test]
async fn a_changed_check_is_pinned_again_only_once_the_researcher_approves_it() {
    use crate::workbench::research;
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let (_temp, m) = coordinator_fixture();
    let folder = tempfile::tempdir().unwrap();
    let operation = store::id();
    let (session, profile) = approved_check(&w, folder.path(), &operation);
    let plan = chain(vec![step(
        "check",
        Action::Check {
            profile_id: profile.clone(),
        },
    )]);
    let scope = adapters::prepare(&plan, Some(session), &json!({}))
        .await
        .unwrap();
    let pinned = scope.checks[&profile].clone();
    let run = m
        .store
        .create(&operation, plan, scope, json!({}), now(), true)
        .unwrap();
    let mut run = m.store.get(&run.id).unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], json!({}));
    run.progress.receipts.get_mut("check").unwrap().state = "failed".into();
    run.state = "attention".into();
    m.store.save(&mut run, "attention", "Changed").unwrap();

    // An edit moves the check past what the researcher approved.
    std::fs::write(folder.path().join("analysis.py"), "print(2)\n").unwrap();
    let held = control(&m, run.id.clone(), run.revision, "refreshContext".into())
        .await
        .unwrap();
    assert_eq!(held.scope.checks[&profile], pinned);
    assert!(held.reason.as_deref().unwrap().contains("Results view"));

    let preview = research::preview_host_execution(&w, &profile).unwrap();
    assert!(!preview.authorized);
    assert_ne!(preview.fingerprint, pinned);
    research::authorize_host_execution(&w, &profile, &preview.fingerprint).unwrap();
    let retried = control(&m, run.id.clone(), held.revision, "retry".into())
        .await
        .unwrap();
    assert_eq!(retried.state, "queued");
    assert_eq!(retried.scope.checks[&profile], preview.fingerprint);
}
