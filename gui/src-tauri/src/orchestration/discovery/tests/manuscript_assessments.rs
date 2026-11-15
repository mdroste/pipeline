use super::*;

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
    assert!(p.papers[0].assessment.as_ref().unwrap().sound);
    assert!(
        !p.papers[0]
            .versions
            .last()
            .unwrap()
            .claim_record
            .as_ref()
            .unwrap()
            .supported
    );
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
fn ranking_uses_assessed_evidence_not_the_assessors_raw_sound_flag() {
    let temp = tempfile::tempdir().unwrap();
    let s = store::Store::open(temp.path()).unwrap();
    let mut p = fixture(Mode::Supervised, 50);
    let mut ranking = Vec::new();
    for (i, key) in ["tableMismatch", "supportedNull"].into_iter().enumerate() {
        let b = crate::orchestration::claims::benchmark::case(key, &s);
        let mut paper = Paper::new(i as u32 + 1);
        paper.assessment=Some(serde_json::from_value(json!({"sound":true,"summary":"Assessor verdict","support":"Evidence","contribution":"Result","completeness":"Draft","originality":"Unknown","remainingWork":[]})).unwrap());
        let v = crate::orchestration::claims::assess(
            format!("v{i}"),
            ("Assessor", "child"),
            &[b.claim],
            &[b.input],
            &b.retained,
            vec![],
            true,
        )
        .unwrap();
        let version:PaperVersion=serde_json::from_value(json!({"version":1,"manuscript":{"title":"Table","abstractText":"Result","markdown":"Paper","latex":"LaTeX","bibliography":"","evidenceIds":[],"limitations":[],"responseToReview":[],"files":[]},"hash":v.subject_version,"artifacts":{},"reviews":[],"externalReview":null,"claimRecord":v})).unwrap();
        paper.versions.push(version);
        p.papers.push(paper);
        ranking.push(RankedPaper {
            candidate_id: i as u32 + 1,
            rank: i + 1,
            reason: "Comparison".into(),
            uncertainty: "Scope".into(),
        });
    }
    assert!(validate::ranking(&ranking, &p).is_err()); // stronger-looking draft without a reproducible estimate
    ranking[0].rank = 2;
    ranking[1].rank = 1;
    validate::ranking(&ranking, &p).unwrap();
}
