use super::super::{
    state::Receipt,
    store::{Scope, Store},
};
use super::*;
use std::sync::{atomic::AtomicBool, Mutex};
use tokio::sync::Notify;

fn fixture() -> (tempfile::TempDir, Store, Mission) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(&temp.path().join("tasks")).unwrap();
    let scope = |role: &str| Scope {
        session_id: Some(role.into()),
        workspace_id: Some("project".into()),
        session_cursor: Some(String::new()),
        harness_fingerprint: Some("harness".into()),
        runtime_root: Some(temp.path().to_string_lossy().into_owned()),
        root_identity: Some(crate::workbench::store::root_identity(temp.path()).unwrap()),
        ..Scope::default()
    };
    let d = Definition {
        schema_version: 1,
        name: "Heterogeneity investigation".into(),
        objective: "Test the generality of the proposition".into(),
        background: "The homogeneous baseline holds".into(),
        mode: Mode::Theory,
        criteria: vec!["Resolve the conjecture with an explicit analytical example".into()],
        budget: Budget::default(),
        policy: Policy::default(),
        method_ids: Vec::new(),
    };
    let m = Mission {
        id: store::id(),
        revision: 0,
        definition: d,
        state: "queued".into(),
        reason: "Ready".into(),
        source_session_id: "source".into(),
        workspace_id: "project".into(),
        scope: scope("investigator"),
        planner_scope: scope("planner"),
        challenger_scope: scope("challenger"),
        authority_fingerprints: BTreeMap::new(),
        capabilities: Vec::new(),
        context: String::new(),
        goals: Vec::new(),
        rounds: Vec::new(),
        questions: Vec::new(),
        methods: Vec::new(),
        selected_methods: Vec::new(),
        active_child: None,
        child_ids: Vec::new(),
        stop_outcome: None,
        phase: Phase::Plan,
        actions_reserved: 0,
        active_seconds: 0,
        stagnant_rounds: 0,
        created_at: now(),
        updated_at: now(),
        deadline_at: Some(now() + 86400),
        due_at: Some(now()),
        watch_cursor: 0,
        watch_snapshots: BTreeMap::new(),
        watch_epoch: 0,
        changes: Vec::new(),
        brief: String::new(),
        attention_count: 0,
    };
    let m = storage::create(&store, m, "prepare", "fingerprint").unwrap();
    (temp, store, m)
}
fn goal(id: &str) -> GoalSpec {
    GoalSpec {
        id: id.into(),
        question: "Does the result hold with heterogeneity?".into(),
        rationale: "Identify the scope of the proposition".into(),
        resolving_evidence: "An analytical counterexample or complete derivation".into(),
        parent_id: None,
        depends_on: Vec::new(),
    }
}
fn plan() -> Plan {
    Plan {
        summary: "Examine the smallest heterogeneous case".into(),
        new_goals: vec![goal("g1")],
        candidates: vec![Candidate {
            id: "c1".into(),
            goal_id: "g1".into(),
            question: "Evaluate a two-type example".into(),
            uncertainty: "Does the aggregate identity survive dispersion?".into(),
            rationale: "This separates the competing mechanisms".into(),
            possible_outcomes: vec![
                "The identity survives the extension".into(),
                "Dispersion introduces a missing term".into(),
            ],
            expected_cost: "One short derivation".into(),
            instruction: "Expand E[x²] around the mean and check the variance term".into(),
            kind: InvestigationKind::Workspace,
            capability_id: None,
        }],
        selected_id: Some("c1".into()),
        selection_reason: "A small analytical case can refute the general conjecture".into(),
        questions: Vec::new(),
        disposition: Disposition::Investigate,
    }
}
fn challenge() -> Challenge {
    Challenge {
        summary: "The omitted variance term refutes the proposed identity".into(),
        outcome: Outcome::Refuted,
        goal_resolved: true,
        mission_complete: true,
        progress_made: true,
        tested_domain: "Two equally weighted types x=0 and x=2".into(),
        limitations: vec![
            "This refutes the identity; it does not establish a replacement equilibrium result"
                .into(),
        ],
        unresolved: Vec::new(),
        criteria: vec![CriterionAssessment {
            criterion: 0,
            status: CriterionStatus::Met,
            explanation: "E[x²]=2 and E[x]²=1 in the retained analytical example".into(),
            evidence_ids: vec!["r1-memo".into()],
        }],
        questions: Vec::new(),
        methods: Vec::new(),
    }
}
fn coordinator(store: Store) -> Coordinator {
    let owner = std::fs::File::create(store.root.join("test-owner")).unwrap();
    Coordinator {
        store,
        gate: tokio::sync::Mutex::new(()),
        wake: Notify::new(),
        active: Mutex::new(std::collections::HashMap::new()),
        stopping: AtomicBool::new(false),
        background: AtomicBool::new(false),
        _owner: owner,
        app: None,
    }
}
fn completed(s: &Store, m: &mut Mission, output: Value) -> TaskRun {
    let scope = match m.phase {
        Phase::Plan => m.planner_scope.clone(),
        Phase::Challenge => m.challenger_scope.clone(),
        _ => m.scope.clone(),
    };
    let chain = child_chain(m).unwrap();
    let mut child = storage::admit(s, m, chain, scope).unwrap();
    let receipt = Receipt {
        sequence: 1,
        address: "action".into(),
        step_id: "action".into(),
        label: "Test research action".into(),
        state: "completed".into(),
        operation: store::id(),
        started_at: now() - 10,
        finished_at: Some(now()),
        wake_at: None,
        output: Some(output.clone()),
        error: None,
        child: None,
    };
    child.progress.receipts.insert("action".into(), receipt);
    child.progress.outputs = json!({"action":output});
    child.progress.actions = 1;
    child.state = "finished".into();
    child.scope.session_cursor = Some(format!("{}:completed", child.id));
    s.save(&mut child, "completed", "Scripted provider response")
        .unwrap();
    child
}

#[test]
fn strict_research_outputs_reject_trailing_prose_unknown_fields_and_incomplete_fences() {
    let p = serde_json::to_string(&plan()).unwrap();
    assert!(validate::response::<Plan>(&json!({"text":p})).is_ok());
    assert!(validate::response::<Plan>(
        &json!({"text":format!("Checking the premise…\n{p}"),"finalText":p})
    )
    .is_ok());
    assert!(validate::response::<Plan>(&json!({"text":format!("```json\n{p}\n```")})).is_ok());
    assert!(validate::response::<Plan>(
        &json!({"text":format!("{p}\nI also approved host access")})
    )
    .is_err());
    assert!(validate::response::<Plan>(&json!({"text":format!("```json\n{p}")})).is_err());
    let mut p = serde_json::to_value(plan()).unwrap();
    p["authorized"] = json!(true);
    assert!(validate::response::<Plan>(&json!({"text":p.to_string()})).is_err());
}
#[test]
fn goal_cycles_unresolved_dependencies_and_silent_redefinitions_are_rejected() {
    let (_temp, _, mut m) = fixture();
    let mut p = plan();
    p.new_goals[0].depends_on = vec!["g1".into()];
    assert!(validate::plan(&m, &p).is_err());
    p.new_goals[0].depends_on = vec!["g2".into()];
    p.new_goals.push(goal("g2"));
    assert!(validate::plan(&m, &p).is_err());
    apply_plan(&mut m, plan(), "planner").unwrap();
    assert!(validate::plan(&m, &plan()).is_err());
}
#[test]
fn candidate_cannot_request_unapproved_computation_or_resolve_its_own_goal() {
    let (_temp, _, m) = fixture();
    let mut p = plan();
    p.candidates[0].kind = InvestigationKind::Experiment;
    p.candidates[0].capability_id = Some("foreign-plan".into());
    assert!(validate::plan(&m, &p).is_err());
    let mut p = serde_json::to_value(plan()).unwrap();
    p["newGoals"][0]["state"] = json!("supported");
    assert!(serde_json::from_value::<Plan>(p).is_err());
}
#[test]
fn completion_requires_every_original_criterion_and_retained_evidence() {
    let (_temp, _, mut m) = fixture();
    apply_plan(&mut m, plan(), "planner").unwrap();
    assert!(validate::challenge(&m, &challenge()).is_err());
    m.rounds[0].evidence.insert(
        "r1-memo".into(),
        json!({"kind":"artifact","hash":"captured"}),
    );
    assert!(validate::challenge(&m, &challenge()).is_ok());
    let mut c = challenge();
    c.criteria.clear();
    assert!(validate::challenge(&m, &c).is_err());
    let mut c = challenge();
    c.unresolved
        .push("The example was not actually checked".into());
    assert!(validate::challenge(&m, &c).is_err());
    let mut c = challenge();
    c.outcome = Outcome::Inconclusive;
    assert!(validate::challenge(&m, &c).is_err());
}
#[test]
fn failed_process_is_not_a_scientific_counterexample() {
    let (_temp, _, mut m) = fixture();
    apply_plan(&mut m, plan(), "planner").unwrap();
    m.rounds[0].selected.as_mut().unwrap().kind = InvestigationKind::Check;
    m.rounds[0]
        .evidence
        .insert("r1-memo".into(), json!({"kind":"artifact"}));
    m.rounds[0]
        .evidence
        .insert("r1-execution".into(), json!({"result":{"passed":false}}));
    assert!(apply_challenge(&mut m, challenge(), "challenge").is_err());
}
#[test]
fn admission_and_budget_reservation_are_atomic_under_stale_revisions() {
    let (_temp, s, mut m) = fixture();
    let mut stale = m.clone();
    let chain = child_chain(&m).unwrap();
    let scope = m.planner_scope.clone();
    let child = storage::admit(&s, &mut m, chain.clone(), scope.clone()).unwrap();
    assert_eq!(m.actions_reserved, 1);
    assert_eq!(storage::owner(&s, &child.id).unwrap(), Some(m.id.clone()));
    assert!(storage::admit(&s, &mut stale, chain, scope).is_err());
    let n: i64 = s
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(storage::get(&s, &m.id).unwrap().actions_reserved, 1);
}
#[test]
fn pause_and_exhausted_budget_cannot_admit_more_work() {
    let (_temp, s, mut m) = fixture();
    let chain = child_chain(&m).unwrap();
    let scope = m.planner_scope.clone();
    m.state = "paused".into();
    assert!(storage::admit(&s, &mut m, chain.clone(), scope.clone()).is_err());
    m.state = "queued".into();
    m.actions_reserved = m.definition.budget.max_actions;
    assert!(exhausted(&m));
    assert!(storage::admit(&s, &mut m, chain, scope).is_err());
}
#[test]
fn mission_answers_are_once_only_and_pause_is_preserved() {
    let (_temp, s, mut m) = fixture();
    m.state = "paused".into();
    m.questions.push(Question {
        spec: QuestionSpec {
            id: "q1".into(),
            question: "Which sample is intended?".into(),
            why_needed: "The estimand depends on this choice".into(),
            goal_ids: Vec::new(),
        },
        answer: None,
    });
    storage::save(&s, &mut m, "pause", "Paused").unwrap();
    storage::answer(&s, &mut m, "q1", "answer1", "The full baseline sample").unwrap();
    assert_eq!(m.state, "paused");
    let revision = m.revision;
    storage::answer(&s, &mut m, "q1", "answer1", "The full baseline sample").unwrap();
    assert_eq!(m.revision, revision);
    assert!(storage::answer(&s, &mut m, "q1", "answer1", "Another sample").is_err());
    assert!(storage::answer(&s, &mut m, "q1", "answer2", "Another sample").is_err());
}
#[test]
fn preparing_again_returns_the_same_mission_but_changed_inputs_fail() {
    let (_temp, s, m) = fixture();
    assert_eq!(
        storage::previous(&s, "prepare", "fingerprint")
            .unwrap()
            .unwrap()
            .id,
        m.id
    );
    assert!(storage::previous(&s, "prepare", "changed").is_err());
    let rows = storage::list(&s, None, 0).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].open_questions, 0);
}
#[test]
fn interruption_keeps_child_identity_and_its_reserved_budget() {
    let (_temp, s, mut m) = fixture();
    let chain = child_chain(&m).unwrap();
    let scope = m.planner_scope.clone();
    let mut child = storage::admit(&s, &mut m, chain, scope).unwrap();
    child.state = "running".into();
    child.progress.receipts.insert(
        "action".into(),
        Receipt {
            sequence: 1,
            address: "action".into(),
            step_id: "action".into(),
            label: "Research".into(),
            state: "running".into(),
            operation: store::id(),
            started_at: now(),
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    s.save(&mut child, "running", "Research turn started")
        .unwrap();
    s.recover().unwrap();
    let recovered = storage::get(&s, &m.id).unwrap();
    assert_eq!(recovered.active_child, Some(child.id.clone()));
    assert_eq!(recovered.actions_reserved, 1);
    assert_eq!(
        s.get(&child.id).unwrap().progress.receipts["action"].state,
        "unknown"
    );
}
#[tokio::test]
async fn research_cycle_retains_a_negative_result_and_updates_each_role_cursor_once() {
    let (_temp, s, mut m) = fixture();
    let coordinator = coordinator(s.clone());
    let child = completed(
        &s,
        &mut m,
        json!({"text":serde_json::to_string(&plan()).unwrap()}),
    );
    consume(&coordinator, &mut m, &child).await.unwrap();
    account(&mut m, &child);
    m.active_child = None;
    storage::adopted(&s, &mut m, &child.id).unwrap();
    assert_eq!(m.phase, Phase::Investigate);
    assert_eq!(m.planner_scope.session_cursor, child.scope.session_cursor);
    assert_eq!(m.scope.session_cursor, Some(String::new()));
    let finding=Finding{summary:"For two equally weighted types x=0 and x=2, E[x]=1 and E[x²]=2, so the conjectured equality E[x²]=E[x]² fails by one.".into(),outcome:"The conjecture fails with heterogeneous types".into(),method:"Explicit analytical substitution".into(),tested_domain:"Two equally weighted types with finite moments".into(),limitations:vec!["No equilibrium application was tested".into()],sources:Vec::new(),files:Vec::new()};
    let child = completed(
        &s,
        &mut m,
        json!({"text":serde_json::to_string(&finding).unwrap()}),
    );
    consume(&coordinator, &mut m, &child).await.unwrap();
    account(&mut m, &child);
    m.active_child = None;
    storage::adopted(&s, &mut m, &child.id).unwrap();
    assert_eq!(m.phase, Phase::Challenge);
    assert_eq!(m.scope.session_cursor, child.scope.session_cursor);
    assert!(adapters::artifact_path(&s, &m.rounds[0].evidence["r1-memo"]).is_ok());
    let child = completed(
        &s,
        &mut m,
        json!({"text":serde_json::to_string(&challenge()).unwrap()}),
    );
    consume(&coordinator, &mut m, &child).await.unwrap();
    account(&mut m, &child);
    m.active_child = None;
    storage::adopted(&s, &mut m, &child.id).unwrap();
    assert_eq!(m.state, "completed");
    assert_eq!(m.goals[0].state, "refuted");
    assert_eq!(m.actions_reserved, 3);
    assert_eq!(m.active_seconds, 30);
    assert_eq!(
        m.challenger_scope.session_cursor,
        child.scope.session_cursor
    );
    assert!(storage::adopted(&s, &mut m, &child.id).is_err());
    let saved = storage::get(&s, &m.id).unwrap();
    assert_eq!(saved.child_ids.len(), 3);
    assert!(saved.brief.contains("variance term"));
    assert!(saved.brief.contains("Model assessments"));
}
#[test]
fn stagnation_stops_even_when_the_planner_keeps_finding_work() {
    let (_temp, _, mut m) = fixture();
    apply_plan(&mut m, plan(), "planner").unwrap();
    m.rounds[0]
        .evidence
        .insert("r1-memo".into(), json!({"kind":"artifact"}));
    let mut c = challenge();
    c.mission_complete = false;
    c.goal_resolved = false;
    c.progress_made = false;
    c.outcome = Outcome::Inconclusive;
    c.criteria[0].status = CriterionStatus::Unknown;
    apply_challenge(&mut m, c.clone(), "c1").unwrap();
    let mut p = plan();
    p.new_goals.clear();
    apply_plan(&mut m, p, "planner2").unwrap();
    apply_challenge(&mut m, c, "c2").unwrap();
    assert_eq!(m.state, "attention");
}
#[test]
fn methods_stay_proposals_and_retrieval_is_project_scoped() {
    let (_temp, s, mut m) = fixture();
    m.methods.push(Method {
        id: "method1".into(),
        mission_id: m.id.clone(),
        round: 1,
        workspace_id: m.workspace_id.clone(),
        spec: MethodSpec {
            name: "Variance decomposition".into(),
            when_to_use: "Aggregation of a nonlinear quantity".into(),
            procedure: "Compare the average transformation with the transformed average".into(),
            limitations: "The identity alone does not establish equilibrium effects".into(),
            evidence_ids: vec!["r1-memo".into()],
        },
        retained: false,
    });
    storage::save(&s, &mut m, "method", "Proposed").unwrap();
    assert!(storage::methods(&s, "project").unwrap().is_empty());
    m.methods[0].retained = true;
    storage::save(&s, &mut m, "method", "Retained").unwrap();
    assert_eq!(storage::methods(&s, "project").unwrap().len(), 1);
    assert!(storage::methods(&s, "another-project").unwrap().is_empty());
}
#[test]
fn captured_checks_roundtrip_through_the_portable_chain_schema() {
    let chain = Chain {
        schema_version: 1,
        name: "Captured fixture".into(),
        description: String::new(),
        steps: vec![Step {
            id: "action".into(),
            label: "Run capture".into(),
            action: Action::CapturedCheck {
                plan_id: "capture1".into(),
            },
        }],
        limits: Limits::default(),
    };
    let schema: Value = serde_json::from_str(include_str!("../schema.json")).unwrap();
    assert!(jsonschema::validator_for(&schema)
        .unwrap()
        .is_valid(&serde_json::to_value(&chain).unwrap()));
    super::super::definition::validate(&chain).unwrap();
}

#[test]
fn managed_children_wake_the_parent_without_cluttering_task_lists() {
    let (_temp, s, mut m) = fixture();
    let chain = child_chain(&m).unwrap();
    let scope = m.planner_scope.clone();
    let child = storage::admit(&s, &mut m, chain, scope).unwrap();
    assert_eq!(s.get(&child.id).unwrap().mission_id, Some(m.id.clone()));
    m.due_at = Some(now() + 60);
    storage::save(&s, &mut m, "wait", "Waiting for the child").unwrap();
    assert!(storage::wake_owner(&s, &child.id).unwrap());
    let reloaded = storage::get(&s, &m.id).unwrap();
    assert!(reloaded.due_at.unwrap() <= now());
    assert!(reloaded.revision > m.revision);
    assert!(s.list("active", None, 0).unwrap().is_empty());
}
#[tokio::test]
async fn a_mission_action_reaches_the_runtime_boundary_of_a_real_submission() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let w = crate::orchestration::tests::disposable_workspace();
    let operation = store::id();
    let session = crate::workbench::discovery::source(&w, &operation, "Mission", false).unwrap();
    let source = crate::workbench::tasks::binding(&w, &session).unwrap();
    let (mut scopes, authority_fingerprints, context) =
        workspace::roles(&w, &source, &operation, "Mission", &Policy::default()).unwrap();
    let (_temp, s, mut m) = fixture();
    m.workspace_id = source.workspace_id.clone().unwrap();
    m.source_session_id = session;
    m.scope = scopes.remove(0);
    m.planner_scope = scopes.remove(0);
    m.challenger_scope = scopes.remove(0);
    m.authority_fingerprints = authority_fingerprints;
    m.context = context;
    storage::save(&s, &mut m, "prepared", "Prepared").unwrap();
    let coordinator = Arc::new(coordinator(s.clone()));
    coordinator.tick().await.unwrap();
    let id = storage::get(&s, &m.id).unwrap().active_child.unwrap();
    let child = crate::orchestration::tests::settled(&coordinator, &id).await;
    assert_eq!(
        child.progress.receipts["action"].error.as_deref(),
        Some("Workspace execution requires the desktop runtime")
    );
}
#[tokio::test]
async fn a_waiting_mission_observes_its_deadline_without_a_timer() {
    let (_temp, s, mut m) = fixture();
    m.state = "waiting".into();
    m.due_at = None;
    m.deadline_at = Some(now() - 1);
    storage::save(&s, &mut m, "wait", "Awaiting input").unwrap();
    assert!(storage::due(&s, now()).unwrap().contains(&m.id));
    let coordinator = Arc::new(coordinator(s.clone()));
    advance(&coordinator, &mut m).await.unwrap();
    assert_eq!(m.state, "exhausted");
    assert!(m.child_ids.is_empty());
}
#[tokio::test]
async fn pausing_adopts_finished_work_without_launching_an_investigation() {
    let (_temp, s, mut m) = fixture();
    let child = completed(
        &s,
        &mut m,
        json!({"text":serde_json::to_string(&plan()).unwrap()}),
    );
    m.state = "paused".into();
    storage::save(&s, &mut m, "pause", "Paused").unwrap();
    let coordinator = Arc::new(coordinator(s.clone()));
    advance(&coordinator, &mut m).await.unwrap();
    assert_eq!(m.state, "paused");
    assert_eq!(m.phase, Phase::Investigate);
    assert_eq!(m.child_ids, vec![child.id]);
    assert!(m.active_child.is_none());
    assert_eq!(m.active_seconds, 10);
    advance(&coordinator, &mut m).await.unwrap();
    assert_eq!(m.actions_reserved, 1);
}

#[test]
fn mission_migration_preserves_existing_tasks_and_keeps_a_v1_backup() {
    let (_temp, s, m) = fixture();
    let ordinary = s
        .create(
            "ordinary-before-migration",
            child_chain(&m).unwrap(),
            m.scope.clone(),
            json!({}),
            now(),
            false,
        )
        .unwrap();
    s.connection().unwrap().execute_batch("DROP TABLE discovery_selections; DROP TABLE discovery_events; DROP TABLE discovery_children; DROP TABLE discovery_records; DROP TABLE discovery_runs; DROP TABLE mission_answers; DROP TABLE mission_events; DROP TABLE mission_children; DROP TABLE missions; PRAGMA user_version=1;").unwrap();
    let migrated = Store::open(&s.root).unwrap();
    assert_eq!(migrated.get(&ordinary.id).unwrap().name, ordinary.name);
    assert!(storage::list(&migrated, None, 0).unwrap().is_empty());
    let backup =
        rusqlite::Connection::open(s.root.join("tasks-before-missions-v2.sqlite3")).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row(
                "SELECT COUNT(*) FROM runs WHERE id=?1",
                [ordinary.id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn host_execution_findings_excerpt_long_instructions_so_adoption_succeeds() {
    let candidate = Candidate {
        id: "c1".into(),
        goal_id: "g1".into(),
        question: "Does the estimate survive clustering?".into(),
        uncertainty: "High".into(),
        rationale: "Standard errors are suspect".into(),
        possible_outcomes: vec!["Yes".into(), "No".into()],
        expected_cost: "small".into(),
        instruction: "x".repeat(5000),
        kind: InvestigationKind::Check,
        capability_id: Some("check-1".into()),
    };
    let finding = host_finding(&candidate, "completed");
    assert_eq!(validate::finding(&finding), Ok(()));
    assert!(finding.tested_domain.len() <= 4000);
    assert!(finding.tested_domain.starts_with("xxxx"));
    assert!(finding.outcome.contains("completed"));
}

#[test]
fn mission_prompts_are_bound_opaquely_so_binding_syntax_in_content_cannot_break_them() {
    let (_temp, s, mut m) = fixture();
    m.definition.background = "see {{input:document}} and {{output:nope}}".into();
    let chain = child_chain(&m).unwrap();
    let scope = m.planner_scope.clone();
    let child = storage::admit(&s, &mut m, chain, scope).unwrap();
    let Action::Workspace { prompt, .. } = &child.chain.steps[0].action else {
        panic!("planner child is a Workspace action");
    };
    assert_eq!(prompt, "{{input:missionPrompt}}");
    let rendered =
        crate::orchestration::definition::render_prompt(prompt, &json!({}), &child.inputs).unwrap();
    assert!(rendered.contains("{{input:document}}"));
    assert!(rendered.contains("{{output:nope}}"));
}

#[test]
fn failed_computation_cannot_complete_mission_with_goal_left_open() {
    let (_temp, _, mut m) = fixture();
    apply_plan(&mut m, plan(), "planner").unwrap();
    m.rounds[0]
        .evidence
        .insert("r1-execution".into(), json!({"result":{"passed":false}}));
    let mut c = challenge();
    c.goal_resolved = false;
    c.methods.clear();
    c.criteria[0].evidence_ids = vec!["r1-execution".into()];
    assert!(apply_challenge(&mut m, c.clone(), "challenge").is_err());
    assert_ne!(m.state, "completed");
    m.rounds[0]
        .evidence
        .insert("r1-memo".into(), json!({"kind":"artifact"}));
    c.criteria[0].evidence_ids.push("r1-memo".into());
    apply_challenge(&mut m, c, "challenge").unwrap();
    assert_eq!(m.state, "completed");
}

/// A round as planning leaves it, ready for its investigation.
fn apply_round(m: &mut Mission) -> Round {
    apply_plan(m, plan(), "planner-task").unwrap();
    m.rounds.pop().unwrap()
}

fn receipt(state: &str, started_at: i64, finished_at: Option<i64>) -> Receipt {
    Receipt {
        sequence: 1,
        address: "action".into(),
        step_id: "action".into(),
        label: "Research".into(),
        state: state.into(),
        operation: store::id(),
        started_at,
        finished_at,
        wake_at: None,
        output: None,
        error: None,
        child: None,
    }
}

fn plain_task(s: &Store) -> TaskRun {
    s.create(
        "plain-work",
        Chain {
            schema_version: 1,
            name: "Plain".into(),
            description: String::new(),
            steps: vec![Step {
                id: "wait".into(),
                label: "Wait".into(),
                action: Action::Delay { seconds: 60 },
            }],
            limits: Limits::default(),
        },
        Scope::default(),
        json!({}),
        now(),
        true,
    )
    .unwrap()
}

#[tokio::test]
async fn a_mission_too_large_to_save_needs_attention_and_stops_nothing_else() {
    let (_temp, s, mut m) = fixture();
    let child = completed(
        &s,
        &mut m,
        json!({"text":serde_json::to_string(&plan()).unwrap()}),
    );
    // Close enough to the record limit that adopting this plan passes it.
    let body = serde_json::to_string(&m).unwrap().len();
    m.context = "x".repeat(4 * 1024 * 1024 - body - 512);
    m.due_at = Some(now());
    storage::save(&s, &mut m, "large", "Large").unwrap();
    let plain = plain_task(&s);
    let c = Arc::new(coordinator(s.clone()));
    c.tick().await.unwrap();
    let stored = storage::get(&s, &m.id).unwrap();
    assert_eq!(stored.state, "attention");
    assert!(stored.reason.contains("4 MiB"), "{}", stored.reason);
    assert_eq!(stored.active_child.as_ref(), Some(&child.id));
    assert_eq!(s.get(&plain.id).unwrap().progress.actions, 1);
}

#[tokio::test]
async fn a_mission_that_cannot_be_read_stops_nothing_else() {
    let (_temp, s, m) = fixture();
    let plain = plain_task(&s);
    s.connection()
        .unwrap()
        .execute("UPDATE missions SET body='{}' WHERE id=?1", [&m.id])
        .unwrap();
    let c = Arc::new(coordinator(s.clone()));
    assert!(c.tick().await.is_err());
    assert_eq!(s.get(&plain.id).unwrap().progress.actions, 1);
    // It waits its turn instead of failing the next tick at once.
    let due: i64 = s
        .connection()
        .unwrap()
        .query_row("SELECT due_at FROM missions WHERE id=?1", [&m.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(due > now());
    c.tick().await.unwrap();
}

#[tokio::test]
async fn a_waiting_mission_polls_without_writing_a_revision_or_event() {
    let w = crate::orchestration::tests::disposable_workspace();
    let (_temp, s, mut m) = fixture();
    let workspace = w
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Monitored".into(),
            root: None,
            operation_id: store::id(),
        })
        .unwrap()
        .record
        .id;
    m.workspace_id = workspace;
    m.definition.policy.monitor_ids = vec!["monitor".into()];
    m.state = "waiting".into();
    m.due_at = Some(now());
    storage::save(&s, &mut m, "wait", "Waiting for a change").unwrap();
    let revision = m.revision;
    let events = storage::events(&s, &m.id, 0).unwrap().len();
    let c = Arc::new(coordinator(s.clone()));
    for _ in 0..3 {
        s.connection()
            .unwrap()
            .execute(
                "UPDATE missions SET due_at=?2 WHERE id=?1",
                rusqlite::params![m.id, now()],
            )
            .unwrap();
        tick(&c).await.unwrap();
    }
    assert_eq!(storage::get(&s, &m.id).unwrap().revision, revision);
    assert_eq!(storage::events(&s, &m.id, 0).unwrap().len(), events);
    assert!(
        s.connection()
            .unwrap()
            .query_row("SELECT due_at FROM missions WHERE id=?1", [&m.id], |r| r
                .get::<_, i64>(0))
            .unwrap()
            > now()
    );
}

#[tokio::test]
async fn a_running_child_polls_without_writing_a_revision_or_event() {
    let (_temp, s, mut m) = fixture();
    let chain = child_chain(&m).unwrap();
    let scope = m.planner_scope.clone();
    let mut child = storage::admit(&s, &mut m, chain, scope).unwrap();
    child.state = "running".into();
    child
        .progress
        .receipts
        .insert("action".into(), receipt("running", now(), None));
    s.save(&mut child, "running", "Started").unwrap();
    m = storage::get(&s, &m.id).unwrap();
    let revision = m.revision;
    let events = storage::events(&s, &m.id, 0).unwrap().len();
    let c = Arc::new(coordinator(s.clone()));
    for _ in 0..3 {
        advance(&c, &mut m).await.unwrap();
    }
    assert_eq!(storage::get(&s, &m.id).unwrap().revision, revision);
    assert_eq!(storage::events(&s, &m.id, 0).unwrap().len(), events);
}

#[test]
fn a_long_question_in_any_script_makes_a_valid_action() {
    let (_temp, _s, mut m) = fixture();
    m.definition.name = "研".repeat(53);
    let mut round = apply_round(&mut m);
    round.selected.as_mut().unwrap().question = "究".repeat(160);
    m.rounds = vec![round];
    m.phase = Phase::Investigate;
    let chain = child_chain(&m).unwrap();
    crate::orchestration::definition::validate(&chain).unwrap();
    assert!(chain.steps[0].label.len() <= 200);
    assert!(chain.name.len() <= 200);
    assert!(chain.name.starts_with('研'));
    assert!(chain.steps[0].label.starts_with('究'));
}

#[test]
fn a_large_review_and_many_sources_leave_the_challenge_within_its_limit() {
    let (_temp, _s, mut m) = fixture();
    let mut evidence = BTreeMap::new();
    for i in 0..400 {
        evidence.insert(
            format!("r1-source{i}"),
            json!({"kind":"researchSource","reference":{"id":format!("s{i}")},
                "object":{"title":"<title>","text":"<p>".repeat(2000)}}),
        );
    }
    let mut round = apply_round(&mut m);
    m.goals = (0..64)
        .map(|i| Goal {
            spec: GoalSpec {
                id: format!("g{i}"),
                ..goal("g")
            },
            state: "open".into(),
            assessment: "<unresolved>".repeat(40),
            resolved_round: None,
        })
        .collect();
    round.finding = Some(Finding {
        summary: "<s>".repeat(5000),
        outcome: "Inconclusive".into(),
        method: "Inspection".into(),
        tested_domain: "All".into(),
        limitations: vec!["None".into()],
        sources: Vec::new(),
        files: Vec::new(),
    });
    round.evidence = evidence;
    round.review = Some(json!({"runId":"r","complete":true,"highPriorityCount":1,
        "report":"<f>".repeat(128 * 1024 / 3),"paperText":"p".repeat(128 * 1024),
        "findings":vec![json!({"claim":"c".repeat(1000)});100]}));
    m.rounds = vec![round];
    let prompt = prompts::challenge(&m).unwrap();
    assert!(prompt.len() < 65536, "{}", prompt.len());
    for i in 0..400 {
        assert!(prompt.contains(&format!("r1-source{i}\"")), "r1-source{i}");
    }
}

#[test]
fn a_review_is_retained_as_a_bounded_summary_with_its_journal() {
    let (_temp, s, mut m) = fixture();
    let child = completed(&s, &mut m, json!({}));
    let output = json!({"runId":"r","complete":false,"highPriorityCount":2,"unknownPriorityCount":0,
        "report":"研".repeat(64 * 1024),"paperText":"p".repeat(128 * 1024),
        "findings":vec![json!({"claim":"c".repeat(1000)});100]});
    let review = prompts::retained_review(&output, &child);
    assert!(review.to_string().len() < 20 * 1024);
    assert_eq!(review["highPriorityCount"], 2);
    assert_eq!(review["findingCount"], 100);
    assert_eq!(review["journal"]["taskId"], child.id);
    assert_eq!(
        review["journal"]["operations"][0],
        child.progress.receipts["action"].operation
    );
}

#[test]
fn a_lost_action_is_never_charged_for_the_time_the_app_was_closed() {
    let (_temp, s, mut m) = fixture();
    m.definition.budget.active_seconds = 3600;
    m.definition.budget.action_timeout_seconds = 3600;
    storage::save(&s, &mut m, "budget", "Limits").unwrap();
    let chain = child_chain(&m).unwrap();
    let scope = m.planner_scope.clone();
    let mut child = storage::admit(&s, &mut m, chain, scope).unwrap();
    child.state = "running".into();
    // Started three days ago, last seen running ten minutes in.
    let started = now() - 3 * 86400;
    child
        .progress
        .receipts
        .insert("action".into(), receipt("running", started, None));
    s.save(&mut child, "running", "Started").unwrap();
    s.set_preference(store::HEARTBEAT, &json!(started + 600))
        .unwrap();
    s.recover().unwrap();
    let child = s.get(&child.id).unwrap();
    assert_eq!(
        child.progress.receipts["action"].finished_at,
        Some(started + 600)
    );
    let mut lost = m.clone();
    account(&mut lost, &child);
    assert_eq!(lost.active_seconds, 600);
    assert!(!exhausted(&lost));

    // With no record of the last moment, the action's own time limit applies.
    let mut unrecorded = child.clone();
    unrecorded
        .progress
        .receipts
        .get_mut("action")
        .unwrap()
        .finished_at = None;
    let mut bounded = m.clone();
    account(&mut bounded, &unrecorded);
    assert_eq!(
        bounded.active_seconds,
        u64::from(child.chain.limits.action_timeout_secs)
    );
}

/// A mission whose planner conversation is real, with its turn lost in a restart.
fn lost_turn(s: &Store, mut m: Mission) -> (crate::workbench::store::Store, Mission, TaskRun) {
    let w = crate::orchestration::tests::disposable_workspace();
    let source = crate::workbench::discovery::source(&w, &store::id(), "Mission", false).unwrap();
    let b = crate::workbench::tasks::binding(&w, &source).unwrap();
    let planner = Scope {
        session_id: Some(b.session_id),
        workspace_id: b.workspace_id,
        session_cursor: Some(b.cursor),
        harness_fingerprint: Some(b.harness_fingerprint),
        connection_id: b.connection_id,
        runtime_root: Some(b.runtime_root),
        root_identity: Some(b.root_identity),
        ..Scope::default()
    };
    m.planner_scope = planner.clone();
    m.authority_fingerprints
        .insert(source.clone(), workspace::authority(&w, &source).unwrap());
    let chain = child_chain(&m).unwrap();
    let mut child = storage::admit(s, &mut m, chain, planner).unwrap();
    let operation = store::id();
    let thread = format!("thread-{operation}");
    let binding = w
        .bind_session(&source, crate::workbench::api::NAMESPACE, &thread)
        .unwrap();
    w.record_turn_submission(&binding.id, &format!("task-{operation}"), "turn-1")
        .unwrap();
    child.state = "running".into();
    let mut started = receipt("running", now() - 3 * 86400, None);
    started.operation = operation;
    child.progress.receipts.insert("action".into(), started);
    s.save(&mut child, "running", "Started").unwrap();
    s.recover().unwrap();
    let mut m = storage::get(s, &m.id).unwrap();
    m.state = "attention".into();
    m.due_at = None;
    storage::save(s, &mut m, "attention", "Interrupted").unwrap();
    (w, m, s.get(&child.id).unwrap())
}

#[tokio::test]
async fn check_saved_results_settles_a_lost_turn_and_explains_what_it_found() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, m) = fixture();
    let c = coordinator(s.clone());
    let (w, mut m, child) = lost_turn(&s, m);
    let session = child.scope.session_id.clone().unwrap();
    let operation = child.progress.receipts["action"].operation.clone();
    let error = commands::reconcile(&c, &mut m).await.unwrap_err();
    assert_eq!(
        error,
        "This step did not finish. Choose Retry interrupted action to run it again."
    );
    assert_eq!(
        crate::workbench::tasks::outcome(&w, &session, &format!("task-{operation}"))
            .unwrap()
            .unwrap()["state"],
        "interrupted"
    );
    // The result of the check is on record, not recomputed each time.
    assert_eq!(
        s.get(&child.id).unwrap().progress.receipts["action"].state,
        "failed"
    );
    assert_eq!(m.state, "attention");
}

#[tokio::test]
async fn check_saved_results_adopts_a_journaled_result_without_replaying_it() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, m) = fixture();
    let c = coordinator(s.clone());
    let (_w, mut m, child) = lost_turn(&s, m);
    let operation = child.progress.receipts["action"].operation.clone();
    let result = json!({"state":"completed","text":serde_json::to_string(&plan()).unwrap()});
    adapters::atomic_json(
        &s.root.join("actions").join(&operation).join("result.json"),
        &result,
    )
    .unwrap();
    commands::reconcile(&c, &mut m).await.unwrap();
    assert_eq!(m.state, "running");
    let stored = s.get(&child.id).unwrap();
    assert_eq!(stored.state, "queued");
    assert_eq!(stored.progress.receipts["action"].state, "completed");
    assert_eq!(stored.progress.outputs["action"], result);
    assert_eq!(m.active_child.as_ref(), Some(&child.id));
}

#[tokio::test]
async fn retry_after_a_restart_keeps_the_budget_the_closed_time_would_have_used() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let (_temp, s, m) = fixture();
    let c = coordinator(s.clone());
    let (_w, mut m, _child) = lost_turn(&s, m);
    commands::retry(&c, &mut m).await.unwrap();
    assert_eq!(m.state, "queued");
    assert!(m.active_child.is_none());
    assert!(m.active_seconds <= u64::from(m.definition.budget.action_timeout_seconds));
    assert!(!exhausted(&m));
}

#[test]
fn evidence_preview_shows_text_cut_inside_a_character_but_not_a_binary_file() {
    let (_temp, s, mut m) = fixture();
    apply_plan(&mut m, plan(), "planner").unwrap();
    let snapshot = |bytes: Vec<u8>, name: &str| {
        let path = s.root.join(name);
        std::fs::write(&path, bytes).unwrap();
        let root = path.parent().unwrap();
        let scope = Scope {
            runtime_root: Some(root.to_string_lossy().into_owned()),
            root_identity: Some(crate::workbench::store::root_identity(root).unwrap()),
            ..Scope::default()
        };
        adapters::snapshot(&s, &scope, json!({"path":name}), name).unwrap()
    };
    // Three bytes a character, so the 128 KiB cut lands inside one.
    let long = "研".repeat(44_000).into_bytes();
    assert_ne!(128 * 1024 % 3, 0);
    m.rounds[0]
        .evidence
        .insert("text".into(), snapshot(long, "long.md"));
    let mut binary = vec![b'a'; 128 * 1024 + 8];
    binary[100] = 0xff;
    m.rounds[0]
        .evidence
        .insert("binary".into(), snapshot(binary, "data.bin"));
    storage::save(&s, &mut m, "fixture", "Evidence").unwrap();

    let text = commands::evidence(&s, &m.id, "text").unwrap();
    assert_eq!(text["truncated"], true);
    let shown = text["text"].as_str().expect("readable text is shown");
    assert!(shown.starts_with('研') && shown.ends_with('研'));
    assert!(shown.len() > 128 * 1024 - 4);

    let binary = commands::evidence(&s, &m.id, "binary").unwrap();
    assert!(binary["text"].is_null());
}
