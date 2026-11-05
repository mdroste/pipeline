//! What Retry and Reconsider the agenda take up, and what they refuse to.
use super::*;
use crate::agent_runtime::events::NormalizedEvent;
use crate::workbench::{research, store as workbench};

/// A mission whose three role conversations are real, in a project with a folder.
struct Roles {
    w: workbench::Store,
    folder: tempfile::TempDir,
    profile: Option<String>,
}

fn with_roles(s: &Store, m: &mut Mission, check: bool) -> Roles {
    let w = crate::orchestration::tests::disposable_workspace();
    let folder = tempfile::tempdir().unwrap();
    let operation = store::id();
    std::fs::write(folder.path().join("analysis.py"), "print(1)\n").unwrap();
    let project = w
        .create_workspace(workbench::CreateWorkspaceRequest {
            name: "Mission".into(),
            root: Some(folder.path().to_string_lossy().into()),
            operation_id: format!("project-{operation}"),
        })
        .unwrap()
        .record
        .id;
    let session = w
        .create_session(workbench::CreateSessionRequest {
            workspace_id: Some(project.clone()),
            title: "Source".into(),
            operation_id: format!("source-{operation}"),
        })
        .unwrap()
        .record
        .id;
    let mut policy = Policy::default();
    let mut profile = None;
    if check {
        let id = research::save_execution_profile(
            &w,
            research::SaveExecutionProfileRequest {
                profile_id: None,
                workspace_id: project.clone(),
                name: "Analysis".into(),
                adapter: "command".into(),
                argv: vec!["/usr/bin/python3".into(), "analysis.py".into()],
                cwd: folder.path().to_string_lossy().into(),
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
        let preview = research::preview_host_execution(&w, &id).unwrap();
        research::authorize_host_execution(&w, &id, &preview.fingerprint).unwrap();
        policy.check_profile_ids = vec![id.clone()];
        profile = Some(id);
    }
    m.capabilities = workspace::capabilities(&w, &project, &policy).unwrap();
    let source = crate::workbench::tasks::binding(&w, &session).unwrap();
    let (mut scopes, fingerprints, context) =
        workspace::roles(&w, &source, &operation, "Mission", &policy).unwrap();
    m.workspace_id = project;
    m.source_session_id = session;
    m.scope = scopes.remove(0);
    m.planner_scope = scopes.remove(0);
    m.challenger_scope = scopes.remove(0);
    for c in &m.capabilities {
        m.scope.checks.insert(c.id.clone(), c.fingerprint.clone());
    }
    m.authority_fingerprints = fingerprints;
    m.context = context;
    storage::save(s, m, "prepared", "Prepared").unwrap();
    Roles { w, folder, profile }
}

/// The researcher sends a message through a role conversation while it is idle.
fn steer(w: &workbench::Store, scope: &Scope) {
    let session = scope.session_id.as_deref().unwrap();
    let thread = format!("thread-{}", store::id());
    let binding = w
        .bind_session(session, crate::workbench::api::NAMESPACE, &thread)
        .unwrap();
    w.record_turn_submission(&binding.id, "researcher-steer", "steer")
        .unwrap();
    w.project_codex_event(
        &binding.id,
        &NormalizedEvent::TurnCompleted {
            epoch: 1,
            thread_id: thread,
            turn_id: "steer".into(),
            status: "completed".into(),
            turn: json!({"id":"steer","status":"completed"}),
        },
    )
    .unwrap();
}

#[tokio::test]
async fn reconsidering_takes_up_every_roles_conversation_not_only_the_planners() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, mut m) = fixture();
    let roles = with_roles(&s, &mut m, false);
    apply_plan(&mut m, plan(), "planner").unwrap();
    m.state = "attention".into();
    m.due_at = None;
    storage::save(&s, &mut m, "attention", "Dispatch failed").unwrap();
    steer(&roles.w, &m.scope);
    steer(&roles.w, &m.challenger_scope);
    let refresh = |scope: &Scope, m: &Mission| {
        let expected = &m.authority_fingerprints[scope.session_id.as_ref().unwrap()];
        workspace::refresh_scope(&roles.w, scope, expected)
    };
    assert!(refresh(&m.scope, &m).is_err());

    let c = coordinator(s.clone());
    commands::retry(&c, &mut m).await.unwrap();
    assert_eq!(m.phase, Phase::Plan);
    assert_eq!(m.state, "queued");
    for scope in [&m.scope, &m.planner_scope, &m.challenger_scope] {
        refresh(scope, &m).unwrap();
    }
}

/// An Investigate round on a check, with its action stopped at the check's gate.
fn stuck_on_check(s: &Store, m: &mut Mission, roles: &Roles) -> TaskRun {
    let id = roles.profile.clone().unwrap();
    let mut selected = plan();
    selected.candidates[0].kind = InvestigationKind::Check;
    selected.candidates[0].capability_id = Some(id);
    apply_plan(m, selected, "planner").unwrap();
    let chain = child_chain(m).unwrap();
    let scope = m.scope.clone();
    let mut child = storage::admit(s, m, chain, scope).unwrap();
    let stopped = Receipt {
        sequence: 1,
        address: "action".into(),
        step_id: "action".into(),
        label: "Run the check".into(),
        state: "unknown".into(),
        operation: store::id(),
        started_at: now() - 5,
        finished_at: Some(now()),
        wake_at: None,
        output: None,
        error: Some("This check's inputs or approval changed.".into()),
        child: None,
    };
    child.progress.receipts.insert("action".into(), stopped);
    child.state = "attention".into();
    s.save(&mut child, "attention", "Changed").unwrap();
    m.state = "attention".into();
    m.due_at = None;
    storage::save(s, m, "attention", "Changed").unwrap();
    child
}

#[cfg(unix)]
#[tokio::test]
async fn a_changed_check_stops_retry_until_approved_and_then_runs_as_approved() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, mut m) = fixture();
    let roles = with_roles(&s, &mut m, true);
    let id = roles.profile.clone().unwrap();
    let child = stuck_on_check(&s, &mut m, &roles);
    let pinned = m.scope.checks[&id].clone();
    std::fs::write(roles.folder.path().join("analysis.py"), "print(2)\n").unwrap();
    let c = coordinator(s.clone());

    let refused = commands::retry(&c, &mut m).await.unwrap_err();
    assert!(refused.contains("Analysis"), "{refused}");
    assert!(refused.contains("Results view"), "{refused}");
    assert_eq!(m.active_child.as_ref(), Some(&child.id));
    assert_eq!(m.scope.checks[&id], pinned);
    assert_eq!(m.actions_reserved, 1);

    let preview = research::preview_host_execution(&roles.w, &id).unwrap();
    research::authorize_host_execution(&roles.w, &id, &preview.fingerprint).unwrap();
    commands::retry(&c, &mut m).await.unwrap();
    assert!(m.active_child.is_none());
    assert_eq!(m.phase, Phase::Investigate);
    assert_eq!(m.scope.checks[&id], preview.fingerprint);
    assert_eq!(m.capabilities[0].fingerprint, preview.fingerprint);
}

#[cfg(unix)]
#[tokio::test]
async fn reconsidering_leaves_a_stuck_check_behind_without_approving_it() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, mut m) = fixture();
    let roles = with_roles(&s, &mut m, true);
    let id = roles.profile.clone().unwrap();
    let child = stuck_on_check(&s, &mut m, &roles);
    let pinned = m.scope.checks[&id].clone();
    std::fs::write(roles.folder.path().join("analysis.py"), "print(2)\n").unwrap();
    steer(&roles.w, &m.scope);
    let c = coordinator(s.clone());

    commands::reconsider(&c, &mut m).await.unwrap();
    assert!(m.active_child.is_none());
    assert_eq!(m.phase, Phase::Plan);
    assert_eq!(m.state, "queued");
    assert_eq!(m.scope.checks[&id], pinned);
    // The stuck action's own scope doesn't come back over the one just taken up.
    let session = m.scope.session_id.as_ref().unwrap();
    workspace::refresh_scope(&roles.w, &m.scope, &m.authority_fingerprints[session]).unwrap();
    // The action was closed out, so it can't be adopted a second time.
    assert!(storage::adopted(&s, &mut m, &child.id).is_err());
}

fn asking(id: &str) -> QuestionSpec {
    QuestionSpec {
        id: id.into(),
        question: "Which sample is intended?".into(),
        why_needed: "The estimand depends on this choice".into(),
        goal_ids: Vec::new(),
    }
}

fn waiting_on(disposition: Disposition, questions: Vec<QuestionSpec>) -> Plan {
    let mut wait = plan();
    wait.selected_id = None;
    wait.disposition = disposition;
    wait.questions = questions;
    wait
}

/// What `pause` leaves when nothing is running.
fn pause(s: &Store, m: &mut Mission) {
    m.state = "paused".into();
    m.reason = "Paused; active work may finish, and no next investigation will start".into();
    m.due_at = None;
    storage::save(s, m, "pause", "Paused").unwrap();
}

#[tokio::test]
async fn resuming_an_unanswered_wait_waits_again_and_spends_nothing() {
    let (_temp, s, mut m) = fixture();
    let wait = waiting_on(Disposition::WaitForInput, vec![asking("q1")]);
    apply_plan(&mut m, wait, "planner").unwrap();
    let reason = m.reason.clone();
    pause(&s, &mut m);
    let c = Arc::new(coordinator(s.clone()));

    commands::resume(&c, &mut m).await.unwrap();
    assert_eq!(m.state, "waiting");
    assert_eq!(m.reason, reason);
    assert_eq!(m.due_at, None);
    storage::save(&s, &mut m, "resume", "Resumed").unwrap();

    advance(&c, &mut m).await.unwrap();
    assert_eq!(m.state, "waiting");
    assert_eq!(m.actions_reserved, 0);
    assert_eq!(m.rounds.len(), 1);
}

#[tokio::test]
async fn an_answer_given_during_the_pause_goes_back_to_the_planner() {
    let (_temp, s, mut m) = fixture();
    let wait = waiting_on(Disposition::WaitForInput, vec![asking("q1"), asking("q2")]);
    apply_plan(&mut m, wait, "planner").unwrap();
    pause(&s, &mut m);
    storage::answer(&s, &mut m, "q1", "answer", "The full baseline sample").unwrap();
    let c = coordinator(s.clone());
    commands::resume(&c, &mut m).await.unwrap();
    assert_eq!(m.state, "queued");
    assert_eq!(m.phase, Phase::Plan);
}

#[tokio::test]
async fn a_change_wait_resumes_waiting_only_if_no_change_ended_it() {
    let (_temp, s, mut m) = fixture();
    m.definition.policy.monitor_ids = vec!["monitor".into()];
    let wait = waiting_on(Disposition::WaitForChange, Vec::new());
    apply_plan(&mut m, wait, "planner").unwrap();
    pause(&s, &mut m);
    let c = coordinator(s.clone());
    commands::resume(&c, &mut m).await.unwrap();
    assert_eq!(m.state, "waiting");
    // Due at once, so the first check reads what changed during the pause.
    assert!(m.due_at.is_some_and(|due| due <= now()));

    // The change had already woken it, and the pause came before the new plan.
    m.watch_epoch += 1;
    m.state = "paused".into();
    commands::resume(&c, &mut m).await.unwrap();
    assert_eq!(m.state, "queued");
}

#[tokio::test]
async fn resuming_a_plan_that_chose_work_does_not_wait() {
    let (_temp, s, mut m) = fixture();
    apply_plan(&mut m, plan(), "planner").unwrap();
    pause(&s, &mut m);
    let c = coordinator(s.clone());
    commands::resume(&c, &mut m).await.unwrap();
    assert_eq!(m.state, "queued");
    assert_eq!(m.phase, Phase::Investigate);
}

#[test]
fn an_interrupted_child_is_announced_by_its_automation_alone() {
    let (_temp, s, mut m) = fixture();
    let chain = child_chain(&m).unwrap();
    let scope = m.scope.clone();
    let mut child = storage::admit(&s, &mut m, chain, scope).unwrap();
    child.progress.receipts.insert(
        "action".into(),
        Receipt {
            state: "running".into(),
            ..receipt("running", now() - 5, None)
        },
    );
    child.state = "running".into();
    s.save(&mut child, "fixture", "In flight").unwrap();
    s.recover().unwrap();
    assert_eq!(s.get(&child.id).unwrap().state, "attention");
    assert!(s.take_notices().unwrap().is_empty());

    let c = coordinator(s.clone());
    c.listening
        .store(false, std::sync::atomic::Ordering::Release);
    m.state = "attention".into();
    notify(&c, &m, "running");
    notify(&c, &m, "running");
    let owed = s.take_notices().unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].event, "missions:notice");
}

/// The planner's turn ended and the agenda needs another one.
fn plan_again(s: &Store, m: &mut Mission) {
    m.active_child = None;
    m.state = "queued".into();
    m.phase = Phase::Plan;
    storage::save(s, m, "fixture", "Planner finished").unwrap();
}

fn planner_prompt(s: &Store, m: &Mission) -> String {
    let child = s.get(m.active_child.as_ref().unwrap()).unwrap();
    child.inputs["missionPrompt"].as_str().unwrap().to_string()
}

#[cfg(unix)]
#[tokio::test]
async fn a_changed_check_stays_out_of_the_plan_and_is_announced_once_until_approved() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, mut m) = fixture();
    let roles = with_roles(&s, &mut m, true);
    let id = roles.profile.clone().unwrap();
    let pinned = m.scope.checks[&id].clone();
    let c = Arc::new(coordinator(s.clone()));
    c.listening
        .store(false, std::sync::atomic::Ordering::Release);

    std::fs::write(roles.folder.path().join("analysis.py"), "print(2)\n").unwrap();
    advance(&c, &mut m).await.unwrap();
    assert!(m.capabilities[0].unavailable);
    assert!(storage::get(&s, &m.id).unwrap().capabilities[0].unavailable);
    assert_eq!(
        m.scope.checks[&id], pinned,
        "an unapproved check keeps its pin"
    );
    let prompt = planner_prompt(&s, &m);
    assert!(prompt.contains(r#""needsApproval":true"#), "{prompt}");
    let owed = s.take_notices().unwrap();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].event, "missions:notice");
    assert_eq!(owed[0].payload["id"], json!(m.id));
    let reason = owed[0].payload["reason"].as_str().unwrap();
    assert!(reason.contains("Analysis"), "{reason}");
    assert!(reason.contains("Results view"), "{reason}");

    // A choice the planner was told not to make stops before it costs an action.
    let mut selected = plan();
    selected.candidates[0].kind = InvestigationKind::Check;
    selected.candidates[0].capability_id = Some(id.clone());
    let refused = apply_plan(&mut m, selected, "planner").unwrap_err();
    assert!(refused.contains("Analysis"), "{refused}");
    assert!(refused.contains("Results view"), "{refused}");
    assert!(m.rounds.is_empty());

    // Still unapproved at the next plan: still marked, and no second notice.
    plan_again(&s, &mut m);
    advance(&c, &mut m).await.unwrap();
    assert!(m.capabilities[0].unavailable);
    assert!(planner_prompt(&s, &m).contains(r#""needsApproval":true"#));
    assert_eq!(m.scope.checks[&id], pinned);
    assert!(s.take_notices().unwrap().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn a_check_approved_again_returns_to_the_plan_and_runs_as_approved() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, mut m) = fixture();
    let roles = with_roles(&s, &mut m, true);
    let id = roles.profile.clone().unwrap();
    let c = Arc::new(coordinator(s.clone()));
    c.listening
        .store(false, std::sync::atomic::Ordering::Release);
    std::fs::write(roles.folder.path().join("analysis.py"), "print(2)\n").unwrap();
    advance(&c, &mut m).await.unwrap();
    assert!(m.capabilities[0].unavailable);
    s.take_notices().unwrap();

    let preview = research::preview_host_execution(&roles.w, &id).unwrap();
    research::authorize_host_execution(&roles.w, &id, &preview.fingerprint).unwrap();
    plan_again(&s, &mut m);
    advance(&c, &mut m).await.unwrap();
    assert!(!m.capabilities[0].unavailable);
    assert_eq!(m.capabilities[0].fingerprint, preview.fingerprint);
    assert_eq!(m.scope.checks[&id], preview.fingerprint);
    assert!(!planner_prompt(&s, &m).contains(r#""needsApproval":true"#));
    assert!(s.take_notices().unwrap().is_empty());

    // The planner may choose it, and the step carries what the gate compares.
    let mut selected = plan();
    selected.candidates[0].kind = InvestigationKind::Check;
    selected.candidates[0].capability_id = Some(id.clone());
    apply_plan(&mut m, selected, "planner").unwrap();
    m.active_child = None;
    storage::save(&s, &mut m, "fixture", "Planner finished").unwrap();
    advance(&c, &mut m).await.unwrap();
    let child = s.get(m.active_child.as_ref().unwrap()).unwrap();
    assert_eq!(child.scope.checks[&id], preview.fingerprint);

    // A later change is news again.
    std::fs::write(roles.folder.path().join("analysis.py"), "print(3)\n").unwrap();
    plan_again(&s, &mut m);
    advance(&c, &mut m).await.unwrap();
    assert!(m.capabilities[0].unavailable);
    assert_eq!(s.take_notices().unwrap().len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn a_check_that_stopped_an_action_is_not_announced_again_after_reconsidering() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, mut m) = fixture();
    let roles = with_roles(&s, &mut m, true);
    stuck_on_check(&s, &mut m, &roles);
    std::fs::write(roles.folder.path().join("analysis.py"), "print(2)\n").unwrap();
    let c = Arc::new(coordinator(s.clone()));
    c.listening
        .store(false, std::sync::atomic::Ordering::Release);
    commands::reconsider(&c, &mut m).await.unwrap();
    assert!(m.capabilities[0].unavailable);
    storage::save(&s, &mut m, "fixture", "Reconsidered").unwrap();
    advance(&c, &mut m).await.unwrap();
    assert!(m.capabilities[0].unavailable);
    assert!(s.take_notices().unwrap().is_empty());
}

#[test]
fn a_prompt_cut_to_fit_still_marks_a_check_that_needs_approval() {
    let (_temp, _s, mut m) = fixture();
    m.capabilities = (0..64)
        .map(|i| Capability {
            id: format!("check-{i}"),
            kind: "check".into(),
            name: format!("Check {i}"),
            fingerprint: "pin".into(),
            parameters: json!({"detail":"x".repeat(1200)}),
            execution: json!({}),
            timeout_seconds: 30,
            unavailable: i == 0,
        })
        .collect();
    let prompt = prompts::plan(&m).unwrap();
    assert!(!prompt.contains("xxxx"), "the capabilities were not cut");
    assert_eq!(prompt.matches(r#""needsApproval":true"#).count(), 1);
}

#[test]
fn a_mission_saved_before_availability_was_tracked_still_reads() {
    let saved = json!({"id":"check","kind":"check","name":"Analysis","fingerprint":"pin",
        "parameters":{},"execution":{},"timeoutSeconds":30});
    let capability: Capability = serde_json::from_value(saved.clone()).unwrap();
    assert!(!capability.unavailable);
    assert_eq!(serde_json::to_value(&capability).unwrap(), saved);
}

#[test]
fn a_new_automation_watches_only_what_comes_after_it_in_its_own_project() {
    let w = crate::orchestration::tests::disposable_workspace();
    let project = |name: &str| {
        w.create_workspace(workbench::CreateWorkspaceRequest {
            name: name.into(),
            root: None,
            operation_id: format!("watch-{}", store::id()),
        })
        .unwrap()
        .record
        .id
    };
    let (ours, other) = (project("Ours"), project("Other"));
    // The baseline reads only this table, so the row needs no monitor behind it.
    let record = |ws: &str| {
        let c = w.connection().unwrap();
        c.pragma_update(None, "foreign_keys", false).unwrap();
        c.execute(
            "INSERT INTO research_attention(id,workspace_id,check_id,fingerprint,body_json,created_at) VALUES(?1,?2,'monitor',?1,'{}',0)",
            [store::id().as_str(), ws],
        )
        .unwrap();
    };
    assert_eq!(workspace::watch_baseline(&w, &ours).unwrap(), 0);
    record(&ours);
    let baseline = workspace::watch_baseline(&w, &ours).unwrap();
    assert!(baseline > 0);
    let monitors = ["monitor".to_string()];
    let (_, before) = workspace::changes(&w, &ours, &monitors, baseline).unwrap();
    assert!(before.is_empty());

    record(&other);
    assert_eq!(workspace::watch_baseline(&w, &ours).unwrap(), baseline);
    record(&ours);
    let (_, after) = workspace::changes(&w, &ours, &monitors, baseline).unwrap();
    assert_eq!(after.len(), 1);
}
