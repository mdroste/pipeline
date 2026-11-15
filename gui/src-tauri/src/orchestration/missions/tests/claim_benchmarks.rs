use super::*;
use crate::orchestration::claims::benchmark::case;

fn run(key: &str) {
    let (_t, s, mut m) = fixture();
    let b = case(key, &s);
    m.definition.criteria = vec![b.claim.clone()];
    apply_plan(&mut m, plan(), "plan").unwrap();
    m.rounds[0].evidence = b.retained;
    let mut c = challenge();
    c.criteria[0].evidence_ids = m.rounds[0].evidence.keys().cloned().collect();
    c.tested_domain = b.scope;
    c.claim_assessments = vec![b.input];
    if key == "infeasible" {
        c.outcome = Outcome::Infeasible;
        c.criteria[0].status = CriterionStatus::Unmet;
        c.mission_complete = false;
        c.unresolved = vec![b.issue];
    } else {
        c.outcome = Outcome::Supported;
    }
    apply_challenge(&mut m, c, "challenge-receipt").unwrap();
    storage::save(&s, &mut m, "assessment", "Benchmark").unwrap();
    let mut saved = storage::get(&s, &m.id).unwrap();
    let r = saved.rounds[0].claim_record.as_mut().unwrap();
    assert_eq!(r.schema_version, 1);
    assert_eq!(r.assessor_child, "challenge-receipt");
    assert_eq!(r.claims[0].claim, b.claim);
    assert_eq!(r.supported, b.grade == "supported");
    assert!(r.accepted_at.is_none());
    match key {
        "missingAssumption" => {
            assert_eq!(saved.state, "attention");
            assert!(saved.brief.contains("full column rank"));
        }
        "tableMismatch" => {
            assert_eq!(saved.state, "attention");
            assert!(saved.brief.contains("differs from reproduced value 2"));
        }
        "supportedNull" => {
            assert_eq!(saved.state, "completed");
            assert_eq!(r.claims[0].evidence.len(), 4);
            assert!(r.accept("round-2").is_err());
            r.accept("round-1").unwrap();
            storage::save(&s, &mut saved, "assessmentAccepted", "Researcher accepted").unwrap();
            assert!(storage::get(&s, &m.id).unwrap().rounds[0]
                .claim_record
                .as_ref()
                .unwrap()
                .accepted_at
                .is_some());
        }
        "infeasible" => {
            assert_eq!(saved.state, "stopped");
            assert!(saved.due_at.is_none());
            assert!(r.accept("round-1").is_err());
        }
        _ => unreachable!(),
    }
}
#[test]
fn missing_assumption_stays_a_named_unresolved_claim() {
    run("missingAssumption")
}
#[test]
fn completed_execution_cannot_rescue_a_nonreproducible_table() {
    run("tableMismatch")
}
#[test]
fn a_supported_sample_null_meets_its_criterion_without_accepting_it() {
    run("supportedNull")
}
#[test]
fn an_infeasible_question_stops_without_claiming_support() {
    run("infeasible")
}
