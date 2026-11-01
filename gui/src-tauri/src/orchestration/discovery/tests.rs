use super::*;
use crate::orchestration::{definition::*, state::Progress};
use std::io::Read as _;

fn fixture(mode: Mode, count: usize) -> Portfolio {
    let mut d = Definition {
        mode,
        candidate_count: count,
        shortlist_count: 3,
        paper_count: 2,
        proposal_reviewers: 1,
        paper_reviewers: 1,
        revision_passes: 1,
        research_rounds: 2,
        max_replacements: 0,
        allow_computation: false,
        ..Definition::default()
    };
    d.prompt =
        "Investigate transmission of manuscript variants in medieval historical sources".into();
    Portfolio {
        run: Run {
            id: store::id(),
            revision: 0,
            definition: d,
            workspace_id: "project".into(),
            source_session_id: "source".into(),
            source_scope: store::Scope::default(),
            source_authority: "authority".into(),
            source_context: String::new(),
            input_artifacts: BTreeMap::new(),
            catalog_revision: crate::auto_review::catalog_revision().into(),
            state: "running".into(),
            reason: String::new(),
            phase: Phase::Orient,
            orientation: None,
            literature: Vec::new(),
            deep_candidates: Vec::new(),
            selection: None,
            selection_hash: None,
            selected: Vec::new(),
            ranking: Vec::new(),
            actions_reserved: 0,
            active_seconds: 0,
            active_child: None,
            active_scope: None,
            replacements: 0,
            repairs: 0,
            created_at: now(),
            updated_at: now(),
            deadline_at: now() + 3600,
            due_at: Some(now()),
            stop_outcome: None,
            selection_since: None,
        },
        candidates: Vec::new(),
        papers: Vec::new(),
    }
}
use super::qualification::response;
fn adopt(p: &mut Portfolio) -> Result<()> {
    let output = json!({"finalText":response(p).to_string()});
    engine::consume(p, &output, BTreeMap::new(), BTreeMap::new())
}
fn until_selection(p: &mut Portfolio) {
    for _ in 0..150 {
        if matches!(p.run.phase, Phase::Select | Phase::Research { .. }) {
            return;
        }
        adopt(p).unwrap();
    }
    panic!("Funnel did not terminate");
}

#[test]
fn complete_unsupervised_funnels_support_all_candidate_counts() {
    for count in [50, 75, 100] {
        let mut p = fixture(Mode::Unsupervised, count);
        for _ in 0..200 {
            if p.run.terminal() {
                break;
            }
            assert_ne!(p.run.phase, Phase::Select);
            adopt(&mut p).unwrap();
        }
        assert_eq!(p.run.state, "completed");
        assert_eq!(p.candidates.len(), count);
        assert_eq!(p.papers.len(), 2);
        assert_eq!(p.run.ranking.len(), 2);
        assert!(p
            .papers
            .iter()
            .all(|p| p.versions.last().unwrap().reviews.len() == 1));
    }
}
#[test]
fn field_orientation_accepts_cross_disciplinary_standards_but_not_unknown_roles() {
    for field in [
        "History",
        "Molecular biology",
        "Mathematics",
        "Literary studies",
        "Engineering",
        "Qualitative sociology",
    ] {
        let mut p = fixture(Mode::Unsupervised, 50);
        let mut value = response(&p);
        value["fields"] = json!([field]);
        engine::consume(
            &mut p,
            &json!({"text":value.to_string()}),
            BTreeMap::new(),
            BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(p.run.orientation.unwrap().fields, [field]);
        value["subjectIds"] = json!(["invented-runtime-command"]);
        assert!(
            validate::orientation(&serde_json::from_value::<Orientation>(value).unwrap()).is_err()
        );
    }
}
#[test]
fn supervised_selection_is_revision_bound_idempotent_and_never_automatic() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Supervised, 50);
    storage::create(&s, &p.run, "start", "fingerprint").unwrap();
    until_selection(&mut p);
    storage::save(&s, &mut p, "shortlist", None).unwrap();
    assert_eq!(p.run.state, "awaitingSelection");
    assert!(p.papers.is_empty());
    assert!(p.run.due_at.is_none());
    assert!(storage::select(&s, &mut p, "stale", vec![1], "pick").is_err());
    let expected = p.run.selection_hash.clone().unwrap();
    assert!(storage::select(&s, &mut p, &expected, vec![1, 1], "pick").is_err());
    assert!(storage::select(&s, &mut p, &expected, vec![1, 2, 3], "pick").is_err());
    storage::select(&s, &mut p, &expected, vec![1, 2], "pick").unwrap();
    let revision = p.run.revision;
    storage::select(&s, &mut p, &expected, vec![1, 2], "pick").unwrap();
    assert_eq!(p.run.revision, revision);
    assert!(storage::select(&s, &mut p, &expected, vec![2], "pick").is_err());
    let restored = storage::load(&store::Store::open(temp.path()).unwrap(), &p.run.id).unwrap();
    assert_eq!(restored.run.selected, [1, 2]);
}
#[test]
fn boundaries_duplicate_links_and_full_batch_coverage_are_enforced() {
    let mut p = fixture(Mode::Unsupervised, 50);
    for invalid in [0, 10, 25] {
        p.run.definition.paper_count = invalid;
        assert!(validate::definition(&p.run.definition).is_err());
    }
    p.run.definition.paper_count = 2;
    validate::definition(&p.run.definition).unwrap();
    p.run.definition.input_paths = vec!["../private.csv".into()];
    assert!(validate::definition(&p.run.definition).is_err());
    p.run.definition.input_paths.clear();
    adopt(&mut p).unwrap();
    adopt(&mut p).unwrap();
    let a: CandidateAssessment =
        serde_json::from_value(super::qualification::assessment(1)).unwrap();
    assert!(validate::assessments(std::slice::from_ref(&a), &[1, 2], &p.candidates).is_err());
    let mut duplicate = a;
    duplicate.duplicate_of = Some(2);
    assert!(validate::assessments(&[duplicate], &[1], &p.candidates).is_err());
}
#[test]
fn revisions_are_re_reviewed_and_final_version_identity_is_preserved() {
    let mut p = fixture(Mode::Unsupervised, 50);
    until_selection(&mut p);
    while !matches!(p.run.phase, Phase::Review { .. }) {
        adopt(&mut p).unwrap();
    }
    let first = p.papers[0].versions[0].hash.clone();
    let v = json!({"summary":"Revise the unsupported interpretation","findings":[{"claim":"Central argument","severity":"major","evidence":"Missing textual distinction","resolution":"Narrow the claim"}],"strengths":[],"limitations":[]});
    engine::consume(
        &mut p,
        &json!({"text":v.to_string()}),
        BTreeMap::new(),
        BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(p.run.phase, Phase::Draft { paper: 0 });
    adopt(&mut p).unwrap();
    assert_eq!(p.run.phase, Phase::Review { paper: 0 });
    assert_ne!(p.papers[0].versions[1].hash, first);
    assert!(p.papers[0].versions[1].reviews.is_empty());
    adopt(&mut p).unwrap();
    assert_eq!(p.run.phase, Phase::AssessPaper { paper: 0 });
    assert_eq!(p.papers[0].versions[0].reviews[0].findings.len(), 1);
}
#[test]
fn unresolved_fatal_findings_cannot_be_overridden_by_final_assessor() {
    let mut p = fixture(Mode::Unsupervised, 50);
    p.run.definition.revision_passes = 0;
    until_selection(&mut p);
    while !matches!(p.run.phase, Phase::Review { .. }) {
        adopt(&mut p).unwrap();
    }
    let v = json!({"summary":"Unsupported central claim","findings":[{"claim":"Conclusion","severity":"fatal","evidence":"No support","resolution":"New evidence required"}],"strengths":[],"limitations":[]});
    engine::consume(
        &mut p,
        &json!({"text":v.to_string()}),
        BTreeMap::new(),
        BTreeMap::new(),
    )
    .unwrap();
    adopt(&mut p).unwrap();
    assert_eq!(p.papers[0].state, "incomplete");
    assert!(!p.papers[0].assessment.as_ref().unwrap().sound);
}
#[test]
fn reviews_can_be_disabled_without_claiming_they_ran() {
    let mut p = fixture(Mode::Unsupervised, 50);
    p.run.definition.paper_reviewers = 0;
    p.run.definition.proposal_reviewers = 0;
    p.run.definition.challenge_research = false;
    for _ in 0..200 {
        if p.run.terminal() {
            break;
        }
        adopt(&mut p).unwrap();
    }
    assert_eq!(p.run.state, "completed");
    assert!(p
        .papers
        .iter()
        .all(|p| p.versions[0].reviews.is_empty() && p.challenges.is_empty()));
}
#[test]
fn aggregate_admission_is_atomic_and_long_prompts_use_bounded_bindings() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Unsupervised, 50);
    storage::create(&s, &p.run, "create", "f").unwrap();
    let chain = Chain {
        schema_version: 1,
        name: "Managed test".into(),
        description: String::new(),
        limits: Limits {
            max_actions: 1,
            deadline_hours: 1,
            action_timeout_secs: 30,
        },
        steps: vec![Step {
            id: "action".into(),
            label: "Test".into(),
            action: Action::Workspace {
                prompt: "x".repeat(90000),
                model: None,
                effort: None,
            },
        }],
    };
    storage::admit(&s, &mut p, chain.clone(), store::Scope::default()).unwrap();
    assert_eq!(p.run.actions_reserved, 1);
    let child = s.get(p.run.active_child.as_ref().unwrap()).unwrap();
    assert_eq!(
        child.inputs["discoveryPrompt"].as_str().unwrap().len(),
        90000
    );
    assert_eq!(
        storage::owner(&s, &child.id).unwrap(),
        Some(p.run.id.clone())
    );
    assert!(s.list("active", None, 0).unwrap().is_empty());
    assert!(storage::admit(&s, &mut p, chain, store::Scope::default()).is_err());
    p.run.active_child = None;
    storage::save(&s, &mut p, "adopt", Some(&child.id)).unwrap();
    assert!(storage::save(&s, &mut p, "adopt", Some(&child.id)).is_err());
}
#[test]
fn interrupted_child_is_retained_without_replay() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Unsupervised, 50);
    storage::create(&s, &p.run, "create", "f").unwrap();
    let chain = Chain {
        schema_version: 1,
        name: "Managed test".into(),
        description: String::new(),
        limits: Limits {
            max_actions: 1,
            deadline_hours: 1,
            action_timeout_secs: 30,
        },
        steps: vec![Step {
            id: "action".into(),
            label: "Test".into(),
            action: Action::Workspace {
                prompt: "Inspect".into(),
                model: None,
                effort: None,
            },
        }],
    };
    storage::admit(&s, &mut p, chain, store::Scope::default()).unwrap();
    let mut child = s.get(p.run.active_child.as_ref().unwrap()).unwrap();
    child.state = "cancelling".into();
    child.progress = Progress::default();
    s.save(&mut child, "interrupted", "Stopped before acknowledgement")
        .unwrap();
    s.recover().unwrap();
    assert_eq!(s.get(&child.id).unwrap().state, "attention");
    assert_eq!(storage::get(&s, &p.run.id).unwrap().actions_reserved, 1);
}
#[test]
fn metadata_and_manuscript_claims_cannot_invent_evidence_ids() {
    let mut p = fixture(Mode::Unsupervised, 50);
    until_selection(&mut p);
    while !matches!(p.run.phase, Phase::Draft { .. }) {
        adopt(&mut p).unwrap();
    }
    let mut draft: Manuscript = serde_json::from_value(response(&p)).unwrap();
    draft.evidence_ids = vec!["invented-execution".into()];
    assert!(validate::manuscript(&draft, &p.papers[0]).is_err());
}
#[test]
fn prompts_adapt_to_orientation_and_referees_do_not_receive_proposal_scores() {
    let mut p = fixture(Mode::Unsupervised, 50);
    until_selection(&mut p);
    while !matches!(p.run.phase, Phase::Review { .. }) {
        adopt(&mut p).unwrap();
    }
    let prompt = prompts::prompt(&p).unwrap();
    assert!(prompt.contains("History"));
    assert!(prompt.contains("Source criticism"));
    assert!(!prompt.contains("eligibleCandidates"));
    assert!(!prompt.contains("informationValue"));
    assert!(!prompt.contains("selectedSourceContext"));
}

#[test]
fn exports_verify_exact_bytes_and_keep_credentials_out_of_the_bundle() {
    use std::io::Read;
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Unsupervised, 50);
    for _ in 0..200 {
        if p.run.terminal() {
            break;
        }
        adopt(&mut p).unwrap();
    }
    let paper = &mut p.papers[0];
    let v = paper.versions.last_mut().unwrap();
    let artifact = adapters::snapshot(
        &s,
        &store::Scope::default(),
        json!(v.manuscript.markdown),
        "paper.md",
    )
    .unwrap();
    v.artifacts.insert("paper.md".into(), artifact.clone());
    p.run.input_artifacts.insert(
        "input-1-source.txt".into(),
        adapters::snapshot(
            &s,
            &store::Scope::default(),
            json!("Original source bytes"),
            "source.txt",
        )
        .unwrap(),
    );
    let bytes = export::bundle(&s, &p).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut markdown = String::new();
    zip.by_name("project-1/paper.md")
        .unwrap()
        .read_to_string(&mut markdown)
        .unwrap();
    assert_eq!(
        markdown,
        p.papers[0].versions.last().unwrap().manuscript.markdown
    );
    assert!(zip.by_name("inputs/input-1-source.txt").is_ok());
    let mut meta = String::new();
    zip.by_name("portfolio.json")
        .unwrap()
        .read_to_string(&mut meta)
        .unwrap();
    assert!(!meta.contains("sourceAuthority"));
    assert!(!meta.contains("runtimeRoot"));
    let path = adapters::artifact_path(&s, &artifact).unwrap();
    std::fs::write(path, "tampered").unwrap();
    assert!(export::bundle(&s, &p).is_err());
}

#[test]
fn proposal_versions_and_budget_reservations_remain_bounded() {
    let mut p = fixture(Mode::Unsupervised, 50);
    until_selection(&mut p);
    for id in &p.run.deep_candidates {
        let c = p.candidates.iter().find(|c| c.id == *id).unwrap();
        assert_eq!(c.previous_versions.len(), 1);
        assert_eq!(c.previous_versions[0].assessments.len(), 2);
        assert_eq!(c.assessments.len(), 1);
    }
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    p.run.actions_reserved = p.run.definition.max_actions;
    storage::create(&s, &p.run, "budget", "fingerprint").unwrap();
    let chain = Chain {
        schema_version: 1,
        name: "Budget test".into(),
        description: String::new(),
        limits: Limits {
            max_actions: 1,
            deadline_hours: 1,
            action_timeout_secs: 30,
        },
        steps: vec![Step {
            id: "action".into(),
            label: "No dispatch".into(),
            action: Action::Workspace {
                prompt: "Cannot spend".into(),
                model: None,
                effort: None,
            },
        }],
    };
    assert!(storage::admit(&s, &mut p, chain, store::Scope::default()).is_err());
    assert!(storage::get(&s, &p.run.id).unwrap().active_child.is_none());
}

#[test]
fn failed_record_adoption_rolls_back_to_the_last_durable_portfolio() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Unsupervised, 50);
    storage::create(&s, &p.run, "bounded", "fingerprint").unwrap();
    until_selection(&mut p);
    storage::save(&s, &mut p, "selected", None).unwrap();
    let committed = p.run.revision;
    p.papers[0].reason = "x".repeat(4 * 1024 * 1024 + 1);
    assert!(storage::save(&s, &mut p, "oversized", None).is_err());
    let restored = storage::load(&s, &p.run.id).unwrap();
    assert_eq!(restored.run.revision, committed);
    assert!(restored.papers[0].reason.is_empty());
}

#[test]
fn only_a_running_action_blocks_a_run_terminally() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Unsupervised, 50);
    storage::create(&s, &p.run, "create", "f").unwrap();
    let chain = Chain {
        schema_version: 1,
        name: "Managed test".into(),
        description: String::new(),
        limits: Limits {
            max_actions: 1,
            deadline_hours: 1,
            action_timeout_secs: 30,
        },
        steps: vec![Step {
            id: "action".into(),
            label: "Test".into(),
            action: Action::Workspace {
                prompt: "Inspect".into(),
                model: None,
                effort: None,
            },
        }],
    };
    storage::admit(&s, &mut p, chain, store::Scope::default()).unwrap();
    let mut child = s.get(p.run.active_child.as_ref().unwrap()).unwrap();
    assert_eq!(failure_state(None, false), "attention");
    assert_eq!(failure_state(Some(&child), false), "attention");
    let receipt = |state: &str| crate::orchestration::state::Receipt {
        sequence: 1,
        address: "action".into(),
        step_id: "action".into(),
        label: "Test".into(),
        state: state.into(),
        operation: store::id(),
        started_at: now(),
        finished_at: None,
        wake_at: None,
        output: None,
        error: None,
        child: None,
    };
    child
        .progress
        .receipts
        .insert("action".into(), receipt("failed"));
    assert_eq!(failure_state(Some(&child), false), "attention");
    child
        .progress
        .receipts
        .insert("action".into(), receipt("unknown"));
    assert_eq!(failure_state(Some(&child), false), "attention");
    assert_eq!(failure_state(Some(&child), true), "settling");
    child
        .progress
        .receipts
        .insert("action".into(), receipt("running"));
    assert_eq!(failure_state(Some(&child), false), "blocked");
    let mut run = p.run.clone();
    run.state = "attention".into();
    assert!(!run.terminal());
    assert!(notice_transition("running", "attention"));
}

#[tokio::test]
async fn recovered_failure_is_durable_before_discovery_classifies_it() {
    use std::sync::{atomic::AtomicBool, Mutex};
    let temp = tempfile::tempdir().unwrap();
    let store = store::Store::open(temp.path()).unwrap();
    let c = Coordinator {
        store,
        gate: tokio::sync::Mutex::new(()),
        wake: tokio::sync::Notify::new(),
        active: Mutex::new(std::collections::HashMap::new()),
        stopping: AtomicBool::new(false),
        background: AtomicBool::new(false),
        _owner: std::fs::File::create(temp.path().join("owner.lock")).unwrap(),
        app: None,
    };
    let chain = Chain {
        schema_version: 1,
        name: "Recovery".into(),
        description: String::new(),
        limits: Limits::default(),
        steps: vec![Step {
            id: "action".into(),
            label: "Action".into(),
            action: Action::Delay { seconds: 1 },
        }],
    };
    let mut child = c
        .store
        .create(
            "recovered-failure",
            chain,
            store::Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    child.state = "attention".into();
    let operation = store::id();
    child.progress.receipts.insert(
        "action".into(),
        crate::orchestration::state::Receipt {
            sequence: 1,
            address: "action".into(),
            step_id: "action".into(),
            label: "Action".into(),
            state: "unknown".into(),
            operation: operation.clone(),
            started_at: now(),
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    c.save(&mut child, "fixture", "fixture").await.unwrap();
    adapters::atomic_json(
        &c.store
            .root
            .join("actions")
            .join(operation)
            .join("result.json"),
        &json!({"state":"failed"}),
    )
    .unwrap();
    c.recover_child(&child).await.unwrap();
    let stored = c.store.get(&child.id).unwrap();
    assert_eq!(stored.progress.receipts["action"].state, "failed");
    assert_eq!(failure_state(Some(&stored), false), "attention");
}

fn coordinator(temp: &tempfile::TempDir) -> Arc<Coordinator> {
    use std::sync::{atomic::AtomicBool, Mutex};
    Arc::new(Coordinator {
        store: store::Store::open(&temp.path().join("tasks")).unwrap(),
        gate: tokio::sync::Mutex::new(()),
        wake: tokio::sync::Notify::new(),
        active: Mutex::new(std::collections::HashMap::new()),
        stopping: AtomicBool::new(false),
        background: AtomicBool::new(false),
        _owner: std::fs::File::create(temp.path().join("owner.lock")).unwrap(),
        app: None,
    })
}

/// A saved run whose source is a real conversation in the disposable Workspace.
fn started(c: &Coordinator) -> (crate::workbench::store::Store, Portfolio) {
    let w = crate::orchestration::tests::disposable_workspace();
    let operation = store::id();
    let session = workspace::source(&w, &operation, "Lost turns", false).unwrap();
    let b = crate::workbench::tasks::binding(&w, &session).unwrap();
    let mut p = fixture(Mode::Unsupervised, 50);
    p.run.workspace_id = b.workspace_id.clone().unwrap();
    p.run.source_authority = research::authority(&w, &session).unwrap();
    p.run.source_session_id = session;
    p.run.source_scope = store::Scope {
        session_id: Some(b.session_id),
        workspace_id: b.workspace_id,
        session_cursor: Some(b.cursor),
        harness_fingerprint: Some(b.harness_fingerprint),
        connection_id: b.connection_id,
        root_identity: Some(b.root_identity),
        runtime_root: Some(b.runtime_root),
        ..store::Scope::default()
    };
    storage::create(&c.store, &p.run, &operation, "fingerprint").unwrap();
    (w, p)
}

/// A run in Research with the author's action admitted and its turn accepted.
fn author_turn(
    c: &Coordinator,
    namespace: &str,
) -> (crate::workbench::store::Store, Portfolio, TaskRun, String) {
    let (w, mut p) = started(c);
    until_selection(&mut p);
    assert_eq!(p.run.phase, Phase::Research { paper: 0 });
    let (scope, authority) = workspace::role(
        &w,
        &p.run.source_scope,
        &p.run.id,
        "paper-1",
        false,
        p.run.deadline_at,
    )
    .unwrap();
    p.papers[0].scope = Some(scope.clone());
    p.papers[0].authority = Some(authority);
    let chain = Chain {
        schema_version: 1,
        name: "Investigate".into(),
        description: String::new(),
        limits: Limits {
            max_actions: 1,
            deadline_hours: 1,
            action_timeout_secs: 1800,
        },
        steps: vec![Step {
            id: "action".into(),
            label: "Investigate".into(),
            action: Action::Workspace {
                prompt: "Inspect".into(),
                model: None,
                effort: None,
            },
        }],
    };
    storage::admit(&c.store, &mut p, chain, scope.clone()).unwrap();
    let mut child = c.store.get(p.run.active_child.as_ref().unwrap()).unwrap();
    let operation = store::id();
    let thread = format!("thread-{operation}");
    let binding = w
        .bind_session(scope.session_id.as_ref().unwrap(), namespace, &thread)
        .unwrap();
    w.record_turn_submission(&binding.id, &format!("task-{operation}"), "turn-1")
        .unwrap();
    child.state = "running".into();
    child.due_at = None;
    child.progress.receipts.insert(
        "action".into(),
        crate::orchestration::state::Receipt {
            sequence: 1,
            address: "action".into(),
            step_id: "action".into(),
            label: "Investigate".into(),
            state: "running".into(),
            operation,
            started_at: now() - 3 * 86400,
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    c.store.save(&mut child, "stepStarted", "Started").unwrap();
    (w, p, child, thread)
}

/// The state the coordinator finds after a restart, once the run comes due.
fn after_restart(c: &Coordinator, p: &mut Portfolio) {
    c.store.recover().unwrap();
    p.run.due_at = Some(now());
    storage::save(&c.store, p, "due", None).unwrap();
}

#[tokio::test]
async fn a_turn_lost_in_a_restart_can_be_retried_without_charging_the_closed_time() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (w, mut p, child, _) = author_turn(&c, crate::workbench::api::NAMESPACE);
    let session = child.scope.session_id.clone().unwrap();
    let submission = format!("task-{}", child.progress.receipts["action"].operation);
    after_restart(&c, &mut p);
    assert_eq!(c.store.get(&child.id).unwrap().state, "attention");

    tick(&c).await.unwrap();
    let mut p = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(p.run.state, "attention");
    assert!(p.run.due_at.is_none());
    assert!(p.run.stop_outcome.is_none());
    assert_eq!(p.run.active_child.as_ref(), Some(&child.id));
    assert_eq!(
        crate::workbench::tasks::outcome(&w, &session, &submission)
            .unwrap()
            .unwrap()["state"],
        "interrupted"
    );
    assert_eq!(
        c.store.get(&child.id).unwrap().progress.receipts["action"].state,
        "failed"
    );
    assert!(workspace::role_policy(&w, &session).unwrap().is_some());

    let adopted = commands::retry(&c, &mut p).await.unwrap();
    assert_eq!(adopted.as_ref(), Some(&child.id));
    save(&c, &mut p, "retry", adopted).await.unwrap();
    assert!(p.run.active_seconds <= 1800);
    tick(&c).await.unwrap();
    let p = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(p.run.state, "running");
    assert_ne!(p.run.active_child.as_ref(), Some(&child.id));
    assert!(p.run.active_child.is_some());
    assert_eq!(p.run.actions_reserved, 2);
}

#[tokio::test]
async fn a_lost_turn_no_runtime_can_settle_still_needs_attention() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    // No desktop handle means the Codex connection is unavailable.
    let (w, mut p, child, _) = author_turn(&c, "codex-app-server-v1");
    let submission = format!("task-{}", child.progress.receipts["action"].operation);
    after_restart(&c, &mut p);
    tick(&c).await.unwrap();
    let p = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(p.run.state, "attention");
    assert!(p.run.stop_outcome.is_none());
    assert_eq!(
        c.store.get(&child.id).unwrap().progress.receipts["action"].state,
        "unknown"
    );
    assert_eq!(
        crate::workbench::tasks::outcome(&w, child.scope.session_id.as_ref().unwrap(), &submission)
            .unwrap()
            .unwrap()["state"],
        "running"
    );
}

/// A timed-out turn, once its interrupt is sent, can still have a live owner
/// for a moment. That is ordinary, so the run waits rather than ending.
#[tokio::test]
async fn a_turn_still_stopping_is_waited_for_and_never_ends_the_run() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (w, mut p, child, thread) = author_turn(&c, "codex-app-server-v1");
    after_restart(&c, &mut p);
    let count = |id: &str| storage::events(&c.store, id, 0).unwrap().len();
    crate::workbench::commands::accept_local_turn(1, &thread, "turn-1");
    let result = tick(&c).await;
    let settling = storage::load(&c.store, &p.run.id).unwrap();
    let events = count(&p.run.id);
    // Later polls change only the next check.
    p.run.revision = settling.run.revision;
    c.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE discovery_runs SET due_at=?2 WHERE id=?1",
            rusqlite::params![p.run.id, now()],
        )
        .unwrap();
    let repeat = tick(&c).await;
    crate::workbench::commands::finish_local_turn(1, &thread, "turn-1");
    result.unwrap();
    repeat.unwrap();
    let again = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(settling.run.state, "running");
    assert!(settling.run.stop_outcome.is_none());
    assert_eq!(settling.run.active_child.as_ref(), Some(&child.id));
    assert_eq!(again.run.revision, settling.run.revision);
    assert_eq!(count(&p.run.id), events);
    drop(w);
}

/// A turn that outlasts the wait goes to attention. It never ends the run.
#[tokio::test]
async fn a_turn_that_never_stops_needs_attention_after_the_wait() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (_w, mut p, child, thread) = author_turn(&c, "codex-app-server-v1");
    after_restart(&c, &mut p);
    c.store
        .connection()
        .unwrap()
        .execute(
            "UPDATE runs SET body=json_set(body,'$.updatedAt',?2) WHERE id=?1",
            rusqlite::params![child.id, now() - SETTLE_SECS - 1],
        )
        .unwrap();
    crate::workbench::commands::accept_local_turn(1, &thread, "turn-1");
    let result = tick(&c).await;
    crate::workbench::commands::finish_local_turn(1, &thread, "turn-1");
    result.unwrap();
    let p = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(p.run.state, "attention");
    assert!(p.run.stop_outcome.is_none());
    assert_eq!(p.run.active_child.as_ref(), Some(&child.id));
}

#[tokio::test]
async fn retry_readopts_the_paper_conversation_after_a_failed_author_turn() {
    use crate::agent_runtime::events::NormalizedEvent;
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (w, mut p, mut child, thread) = author_turn(&c, crate::workbench::api::NAMESPACE);
    let session = child.scope.session_id.clone().unwrap();
    let binding = w.active_binding(&session).unwrap().unwrap();
    w.project_codex_event(
        &binding.id,
        &NormalizedEvent::TurnCompleted {
            epoch: 1,
            thread_id: thread,
            turn_id: "turn-1".into(),
            status: "failed".into(),
            turn: json!({"id":"turn-1","status":"failed"}),
        },
    )
    .unwrap();
    let receipt = child.progress.receipts.get_mut("action").unwrap();
    receipt.state = "failed".into();
    receipt.started_at = now() - 90;
    receipt.finished_at = Some(now());
    child.state = "attention".into();
    c.store.save(&mut child, "attention", "Failed").unwrap();
    p.run.state = "attention".into();
    p.run.due_at = None;
    storage::save(&c.store, &mut p, "attention", None).unwrap();
    let stale = p.papers[0].scope.clone().unwrap();
    let authority = p.papers[0].authority.clone().unwrap();
    assert!(research::refresh_scope(&w, &stale, &authority).is_err());

    let adopted = commands::retry(&c, &mut p).await.unwrap();
    assert_eq!(adopted.as_ref(), Some(&child.id));
    save(&c, &mut p, "retry", adopted).await.unwrap();
    assert!((90..200).contains(&p.run.active_seconds));
    assert!(p.run.active_child.is_none());
    assert!(storage::save(&c.store, &mut p, "again", Some(&child.id)).is_err());

    tick(&c).await.unwrap();
    let p = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(p.run.state, "running", "{}", p.run.reason);
    assert_ne!(p.run.active_child.as_ref(), Some(&child.id));
    assert!(p.run.active_child.is_some());
    assert_eq!(p.run.actions_reserved, 2);
}

#[tokio::test]
async fn a_discovery_action_reaches_the_runtime_boundary_of_a_real_submission() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (_w, p) = started(&c);
    c.tick().await.unwrap();
    let dispatched = storage::load(&c.store, &p.run.id).unwrap();
    let id = dispatched.run.active_child.as_ref().unwrap();
    let child = crate::orchestration::tests::settled(&c, id).await;
    assert_eq!(
        child.progress.receipts["action"].error.as_deref(),
        Some("Workspace execution requires the desktop runtime")
    );
    // Nothing was sent, so a Retry remains available.
    tick(&c).await.unwrap();
    let p = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(p.run.state, "attention");
}

#[tokio::test]
async fn the_final_delivery_tolerates_a_draft_and_moved_research_context() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (w, mut p) = started(&c);
    for _ in 0..300 {
        if p.run.phase == Phase::Deliver {
            break;
        }
        adopt(&mut p).unwrap();
    }
    assert_eq!(p.run.phase, Phase::Deliver);
    p.run.definition.review_profile_id = None;
    storage::save(&c.store, &mut p, "ready", None).unwrap();
    let session = p.run.source_session_id.clone();
    // The conversation's research context moves, and the researcher starts a draft.
    let before = crate::workbench::tasks::binding(&w, &session).unwrap();
    crate::workbench::tasks::deliver(&w, &session, &store::id(), json!({"report":"Earlier"}))
        .unwrap();
    let current = w.session_snapshot(&session).unwrap().session;
    w.update_session(crate::workbench::store::UpdateSessionRequest {
        session_id: session.clone(),
        expected_revision: current.revision,
        operation_id: store::id(),
        title: None,
        draft: Some("An unsent idea".into()),
        overrides: None,
        archived: None,
        preset_id: None,
        paper_id: None,
        clear_paper: None,
    })
    .unwrap();
    let after = crate::workbench::tasks::binding(&w, &session).unwrap();
    assert_ne!(before.harness_fingerprint, after.harness_fingerprint);

    c.tick().await.unwrap();
    let dispatched = storage::load(&c.store, &p.run.id).unwrap();
    let id = dispatched.run.active_child.clone().unwrap();
    let child = crate::orchestration::tests::settled(&c, &id).await;
    assert_eq!(
        child.progress.receipts["action"].state, "completed",
        "{:?}",
        child.progress.receipts["action"].error
    );
    for _ in 0..4 {
        c.tick().await.unwrap();
    }
    let done = storage::load(&c.store, &p.run.id).unwrap();
    assert!(
        matches!(done.run.state.as_str(), "completed" | "partial"),
        "{}: {}",
        done.run.state,
        done.run.reason
    );
    assert_eq!(
        w.session_snapshot(&session).unwrap().session.draft,
        "An unsent idea"
    );
}

#[test]
fn the_shortlist_waits_past_the_deadline_and_keeps_the_time_left() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Supervised, 50);
    storage::create(&s, &p.run, "start", "fingerprint").unwrap();
    until_selection(&mut p);
    assert_eq!(p.run.state, "awaitingSelection");
    let waited = 10 * 3600;
    let left = p.run.deadline_at - now();
    p.run.selection_since = Some(now() - waited);
    p.run.deadline_at = now() + left - waited;
    assert!(p.run.deadline_at <= now());
    storage::save(&s, &mut p, "shortlist", None).unwrap();
    assert!(!p.run.deadline_passed());
    // Past the old deadline, the run is not due at all.
    assert!(storage::due(&s).unwrap().is_empty());
    assert!(
        s.next_due().unwrap().is_none_or(|t| t > now() + 30),
        "the coordinator must not spin"
    );
    let shortlist = p.run.selection.as_ref().unwrap().shortlist.clone();
    let chosen = vec![shortlist[0]];
    let hash = p.run.selection_hash.clone().unwrap();
    storage::select(&s, &mut p, &hash, chosen, "choose").unwrap();
    assert_eq!(p.run.state, "running");
    assert!(p.run.selection_since.is_none());
    assert!((left - 5..=left + 5).contains(&(p.run.deadline_at - now())));
    assert!(!p.run.deadline_passed());
}

#[tokio::test]
async fn a_paused_shortlist_resumes_past_the_old_deadline_and_stops_polling() {
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let mut p = fixture(Mode::Supervised, 50);
    storage::create(&c.store, &p.run, "start", "fingerprint").unwrap();
    until_selection(&mut p);
    p.run.selection_since = Some(now() - 100);
    p.run.deadline_at = now() - 1;
    // A resumed shortlist comes due once, and nothing else happens until a choice.
    p.run.due_at = Some(now());
    storage::save(&c.store, &mut p, "resume", None).unwrap();
    let revision = p.run.revision;
    tick(&c).await.unwrap();
    let stored = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(stored.run.state, "awaitingSelection");
    assert_eq!(stored.run.revision, revision);
    assert!(storage::due(&c.store).unwrap().is_empty());
}

#[tokio::test]
async fn a_running_action_polls_without_writing_a_revision_or_event() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (_w, mut p, _child, _) = author_turn(&c, crate::workbench::api::NAMESPACE);
    let revision = p.run.revision;
    let events = storage::events(&c.store, &p.run.id, 0).unwrap().len();
    for _ in 0..3 {
        advance(&c, &mut p).await.unwrap();
    }
    assert_eq!(
        storage::get(&c.store, &p.run.id).unwrap().revision,
        revision
    );
    assert_eq!(
        storage::events(&c.store, &p.run.id, 0).unwrap().len(),
        events
    );
}

#[tokio::test]
async fn a_reply_too_long_to_use_gets_a_repair_turn_and_keeps_its_cursor() {
    let _turn = crate::orchestration::tests::exclusive_turn_state().await;
    let temp = tempfile::tempdir().unwrap();
    let c = coordinator(&temp);
    let (_w, mut p, child, _) = author_turn(&c, crate::workbench::api::NAMESPACE);
    let reply = json!({"state":"completed","replyTooLong":true,"cursor":"turn:completed","text":"Part of it"});
    c.complete(&child.id, "action", Ok(reply)).await.unwrap();
    let failed = c.store.get(&child.id).unwrap();
    assert_eq!(failed.state, "attention");
    assert_eq!(failed.progress.receipts["action"].state, "failed");
    assert_eq!(
        failed.scope.session_cursor.as_deref(),
        Some("turn:completed")
    );
    p = storage::load(&c.store, &p.run.id).unwrap();
    advance(&c, &mut p).await.ok();
    let stored = storage::load(&c.store, &p.run.id).unwrap();
    assert_eq!(stored.run.state, "running", "{}", stored.run.reason);
    assert_eq!(stored.run.repairs, 1);
    assert!(
        stored.run.reason.contains("too long"),
        "{}",
        stored.run.reason
    );
    assert_eq!(
        stored.papers[0]
            .scope
            .as_ref()
            .unwrap()
            .session_cursor
            .as_deref(),
        Some("turn:completed")
    );
}

#[test]
fn a_long_paper_history_is_saved_with_older_versions_read_back_from_their_files() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Unsupervised, 50);
    for _ in 0..200 {
        if p.run.terminal() {
            break;
        }
        adopt(&mut p).unwrap();
    }
    let template = p.papers[0].versions[0].clone();
    let large = |n: usize, c: &str| c.repeat(n * 1024 * 1024 / c.len());
    let (first, second, newest) = (large(3, "first "), large(1, "second "), large(1, "newest "));
    p.papers[0].versions = [&first, &second, &newest]
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let mut version = template.clone();
            version.version = index + 1;
            version.manuscript.markdown = text.clone();
            version.manuscript.bibliography = String::new();
            for (name, content) in [
                ("paper.md", text.as_str()),
                ("paper.tex", version.manuscript.latex.as_str()),
                ("references.bib", ""),
            ] {
                let artifact =
                    adapters::snapshot(&s, &store::Scope::default(), json!(content), name).unwrap();
                version.artifacts.insert(name.into(), artifact);
            }
            version
        })
        .collect();
    storage::create(&s, &p.run, "start", "fingerprint").unwrap();
    storage::save(&s, &mut p, "adopted", None).unwrap();

    let stored = storage::load(&s, &p.run.id).unwrap();
    let versions = &stored.papers[0].versions;
    assert!(versions[0].manuscript.markdown.is_empty());
    assert_eq!(versions[2].manuscript.markdown, newest);
    let full = super::artifacts::full_versions(&s, &stored.papers[0]);
    assert_eq!(full[0].manuscript.markdown, first);
    assert_eq!(full[0].manuscript.latex, template.manuscript.latex);
    assert_eq!(full[1].manuscript.markdown, second);
    assert_eq!(full[0].hash, template.hash);

    let bytes = export::bundle(&s, &stored).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut research = String::new();
    zip.by_name("project-1/research.json")
        .unwrap()
        .read_to_string(&mut research)
        .unwrap();
    assert!(research.contains("first first"));
}

#[test]
fn a_paper_stays_refused_when_its_history_is_not_what_exceeds_the_limit() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Unsupervised, 50);
    for _ in 0..200 {
        if p.run.terminal() {
            break;
        }
        adopt(&mut p).unwrap();
    }
    p.papers[0]
        .evidence
        .insert("large".into(), json!("x".repeat(5 * 1024 * 1024)));
    storage::create(&s, &p.run, "start", "fingerprint").unwrap();
    assert!(storage::save(&s, &mut p, "adopted", None)
        .unwrap_err()
        .contains("4 MiB"));
}

#[test]
fn the_exported_report_uses_the_name_on_screen() {
    let p = fixture(Mode::Supervised, 50);
    assert!(export::report(&p).starts_with("# Research discovery — choose projects yourself\n"));
    let p = fixture(Mode::Unsupervised, 50);
    assert!(export::report(&p).starts_with("# Research discovery — let Pipeline choose projects\n"));
}
