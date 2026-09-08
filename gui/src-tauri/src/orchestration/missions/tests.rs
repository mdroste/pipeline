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
    s.connection().unwrap().execute_batch("DROP TABLE mission_answers; DROP TABLE mission_events; DROP TABLE mission_children; DROP TABLE missions; PRAGMA user_version=1;").unwrap();
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
