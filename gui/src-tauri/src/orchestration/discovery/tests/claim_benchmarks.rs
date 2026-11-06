use super::*;
use crate::orchestration::claims::benchmark::case;

fn send(
    p: &mut Portfolio,
    value: Value,
    evidence: BTreeMap<String, Value>,
    files: BTreeMap<String, Value>,
) {
    p.run.active_child = Some(format!("child-{}", export::phase_name(&p.run.phase)));
    engine::consume(p, &json!({"finalText":value.to_string()}), evidence, files).unwrap();
    p.run.active_child = None;
}
fn run(key: &str) {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let b = case(key, &s);
    let mut p = fixture(Mode::Unsupervised, 50);
    p.run.definition.paper_count = 1;
    p.run.definition.challenge_research = true;
    p.run.definition.paper_reviewers = 0;
    until_selection(&mut p);
    let mut value = response(&p);
    value["claims"] = json!([b.claim]);
    value["contract"] = json!(b.scope);
    send(&mut p, value, b.retained, BTreeMap::new());
    let mut value = response(&p);
    value["assessment"] = json!(if key == "infeasible" {
        b.issue.as_str()
    } else {
        "Check the draft against the retained evidence"
    });
    value["unresolved"] = json!(if b.issue.is_empty() {
        vec![]
    } else {
        vec![b.issue.clone()]
    });
    value["next"] = json!(if key == "infeasible" {
        "abandon"
    } else {
        "draft"
    });
    send(&mut p, value, BTreeMap::new(), BTreeMap::new());
    if key == "infeasible" {
        assert!(matches!(p.run.phase, Phase::Deliver));
        let r = p.papers[0].infeasibility.as_ref().unwrap();
        assert!(!r.supported && r.accepted_at.is_none());
        assert!(r.describe().contains(&b.issue));
    } else {
        let mut v = response(&p);
        v["centralClaims"] = json!([b.claim]);
        v["markdown"] = json!(format!("# Draft\n\n{}", b.claim));
        let doc = adapters::snapshot(
            &s,
            &store::Scope::default(),
            v["markdown"].clone(),
            "paper.md",
        )
        .unwrap();
        send(
            &mut p,
            v,
            BTreeMap::new(),
            BTreeMap::from([("paper.md".into(), doc)]),
        );
        let mut verdict = response(&p);
        verdict["claimAssessments"] = json!([b.input]);
        // A favorable assessor verdict is insufficient in the first two cases.
        assert_eq!(verdict["sound"], true);
        send(&mut p, verdict, BTreeMap::new(), BTreeMap::new());
        let v = p.papers[0].versions.last().unwrap();
        let r = v.claim_record.as_ref().unwrap();
        assert_eq!((r.supported, r.accepted_at), (b.grade == "supported", None));
        assert_eq!(r.subject_version, v.hash);
        assert_eq!(r.assessor_child, "child-Assess final paper");
        assert_eq!(v.draft_delivered, Some(false));
        if key == "tableMismatch" {
            assert!(r.describe().contains("differs from reproduced value 2"));
        }
        if key == "missingAssumption" {
            assert!(r.describe().contains("full column rank"));
        }
        adopt(&mut p).unwrap(); // rank
        let payload = export::delivery(&p);
        assert_eq!(payload["papers"][0]["draftIncluded"], true);
        assert_eq!(
            payload["papers"][0]["claimRecord"]["supported"],
            b.grade == "supported"
        );
    }
    adopt(&mut p).unwrap(); // local delivery acknowledged
    if key == "infeasible" {
        assert_eq!(p.run.state, "partial");
        assert!(p.papers[0].versions.is_empty());
    } else {
        assert_eq!(p.run.state, "completed"); // delivery was the requested action
        let v = &mut p.papers[0].versions[0];
        assert_eq!(v.draft_delivered, Some(true));
        let record = v.claim_record.as_mut().unwrap();
        assert!(record.accept("wrong-version").is_err());
        assert_eq!(record.accept(&v.hash).is_ok(), b.grade == "supported");
        let view = export::view(&p);
        assert_eq!(view["papers"][0]["draftDelivered"], true);
        let bytes = export::bundle(&s, &p).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let saved: Value = serde_json::from_reader(
            zip.by_name(&format!(
                "project-{}/research.json",
                p.papers[0].candidate_id
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            saved["versions"][0]["claimRecord"],
            view["papers"][0]["claimRecord"]
        );
    }
}
#[test]
fn a_proof_with_a_missing_assumption_can_deliver_only_a_draft() {
    run("missingAssumption")
}
#[test]
fn an_irreproducible_coefficient_remains_named_after_delivery() {
    run("tableMismatch")
}
#[test]
fn an_inspected_null_is_supported_and_only_a_researcher_accepts_it() {
    run("supportedNull")
}
#[test]
fn an_infeasible_project_stops_without_a_draft_or_false_completion() {
    run("infeasible")
}

#[test]
fn earlier_assessment_does_not_gain_support_acceptance_or_a_delivery_timestamp() {
    let old = json!({"version":1,"manuscript":{"title":"Old","abstractText":"Earlier abstract","markdown":"Old draft","latex":"","bibliography":"","evidenceIds":[],"limitations":[],"responseToReview":[],"files":[]},"hash":"old-hash","artifacts":{},"reviews":[],"externalReview":null});
    let v: PaperVersion = serde_json::from_value(old).unwrap();
    let mut paper = Paper::new(1);
    paper.state = "complete".into();
    paper.versions.push(v);
    paper.assessment=Some(serde_json::from_value(json!({"summary":"Older judgment","sound":true,"contribution":"Contribution","support":"Earlier assessor","originality":"Unknown","completeness":"Draft","remainingWork":[]})).unwrap());
    let mut p = fixture(Mode::Supervised, 50);
    p.papers.push(paper);
    let view = export::view(&p);
    assert_eq!(view["papers"][0]["assessment"]["sound"], true);
    assert!(
        view["papers"][0]["draftDelivered"].is_null() && view["papers"][0]["claimRecord"].is_null()
    );
    assert!(export::report(&p).contains("Earlier assessment"));
}
