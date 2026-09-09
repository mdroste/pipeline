use super::*;
use crate::orchestration::{definition::*, state::Progress};

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
        },
        candidates: Vec::new(),
        papers: Vec::new(),
    }
}
fn assessment(id: u32) -> Value {
    json!({"candidateId":id,"eligible":true,"contribution":4,"feasibility":4,"informationValue":3,"cluster":format!("cluster-{id}"),"duplicateOf":null,"strongestObjection":"The relation between sources remains uncertain","resolution":"Compare the preserved variants and provenance","uncertainty":"Incomplete archive coverage"})
}
fn response(p: &Portfolio) -> Value {
    match p.run.phase {
        Phase::Orient => {
            json!({"fields":["History"],"subjectIds":[],"methodIds":[],"researchStandards":["Source criticism and provenance"],"contributionForms":["Interpretation supported by primary texts"],"proposalLenses":["Transmission","Attribution","Context"],"assumptions":[],"constraints":["Only available primary sources"],"literatureQueries":[]})
        }
        Phase::Generate => {
            json!({"proposals":(0..BATCH.min(p.run.definition.candidate_count-p.candidates.len())).map(|i|json!({"title":format!("Project {}",p.candidates.len()+i+1),"question":"What explains the textual variant?","contribution":"An account of source transmission","method":"Historical source criticism","firstTest":"Compare independently dated witnesses","requiredEvidence":["Primary texts"],"relatedWork":[],"risks":["Missing witness"]})).collect::<Vec<_>>()})
        }
        Phase::Screen { offset } => {
            json!({"assessments":p.candidates.iter().skip(offset).take(BATCH).map(|c|assessment(c.id)).collect::<Vec<_>>()})
        }
        Phase::Assess { index, .. } => {
            json!({"assessments":[assessment(p.run.deep_candidates[index])]})
        }
        Phase::ReviseProposal { index } => json!(
            p.candidates
                .iter()
                .find(|c| c.id == p.run.deep_candidates[index])
                .unwrap()
                .proposal
        ),
        Phase::Shortlist => {
            json!({"shortlist":p.run.deep_candidates.iter().take(3).collect::<Vec<_>>(),"recommended":p.run.deep_candidates.iter().take(2).collect::<Vec<_>>(),"rationale":"Distinct source questions with feasible comparisons"})
        }
        Phase::Research { .. } => {
            json!({"contract":"Assess transmission under explicit source limitations","summary":"The available witnesses support a bounded interpretation","claims":["The interpretation is provisional"],"limitations":["Missing witnesses prevent a definitive claim"],"sources":[],"files":[],"next":"draft","reason":"The bounded argument can be written with limitations"})
        }
        Phase::Challenge { .. } => {
            json!({"assessment":"The bounded argument supports a draft","unresolved":["Source coverage"],"progress":true,"next":"draft"})
        }
        Phase::Draft { paper } => {
            json!({"title":format!("Paper {}",p.papers[paper].candidate_id),"abstractText":"A bounded interpretation of source transmission","markdown":format!("# Paper {}\n\nA bounded argument. Version {}",p.papers[paper].candidate_id,p.papers[paper].versions.len()+1),"latex":"\\documentclass{article}\\begin{document}A bounded argument.\\end{document}","bibliography":"","evidenceIds":[],"limitations":["Source coverage"],"responseToReview":[],"files":[]})
        }
        Phase::Review { .. } => {
            json!({"summary":"The central claim is appropriately bounded","findings":[],"strengths":["Explicit source criticism"],"limitations":["Coverage"]})
        }
        Phase::AssessPaper { .. } => {
            json!({"summary":"A coherent bounded paper draft","sound":true,"contribution":"Useful interpretation","support":"Argument supported within the provided record","originality":"Uncertain beyond the inspected literature","completeness":"Complete bounded draft","remainingWork":["Broader archive access"]})
        }
        Phase::Rank => {
            json!({"papers":p.papers.iter().enumerate().filter(|(_,p)|p.assessment.is_some()).map(|(i,p)|json!({"candidateId":p.candidate_id,"rank":i+1,"reason":"Stronger interpretive contribution","uncertainty":"A close comparison"})).collect::<Vec<_>>()})
        }
        Phase::Deliver => json!({"delivered":true}),
        _ => panic!("No scripted response for {:?}", p.run.phase),
    }
}
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
    let a: CandidateAssessment = serde_json::from_value(assessment(1)).unwrap();
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
