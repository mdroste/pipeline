//! Refuse unsupported legacy or fabricated evidence even with a favorable assessor.
use super::*;
#[test]
fn an_unrelated_successful_check_and_a_misquoted_output_cannot_support_the_table() {
    let t = tempfile::tempdir().unwrap();
    let s = crate::orchestration::store::Store::open(t.path()).unwrap();
    let mut b = benchmark::case("supportedNull", &s);
    b.retained.get_mut("r1-execution").unwrap()["result"]["receipt"]["inputManifest"]["files"][0]
        ["contentHash"] = json!("another-sample");
    b.retained.get_mut("r1-execution").unwrap()["result"]["receipt"]["outputManifest"]["slope"] =
        json!(3);
    let r = assess(
        "v1".into(),
        ("Assessor", "child"),
        &[b.claim],
        &[b.input],
        &b.retained,
        vec![],
        true,
    )
    .unwrap();
    assert!(!r.supported);
    assert!(r.describe().contains("links the inspected data"));
    assert!(r
        .describe()
        .contains("disagrees with the cited execution output"));
}
#[test]
fn an_interpretation_needs_a_source_version_and_an_inspected_passage() {
    let mut input:Submission=serde_json::from_value(json!({"subject":0,"kind":"interpretation","claim":"The letter qualified the proposed reform.","scope":"This letter alone","verdict":"supported","inspections":[{"role":"passage","evidenceId":"r1-source1","location":"paragraph 2","observed":"The writer explicitly excepted existing leases."}],"objections":[]})).unwrap();
    let retained = BTreeMap::from([
        (
            "r1-source1".into(),
            json!({"kind":"researchSource","reference":{"kind":"source","id":"letter","revision":"rev1"},"object":{"access":"full","text":"Except existing leases."}}),
        ),
        (
            "r1-memo".into(),
            json!({"kind":"artifact","hash":"investigator-memo"}),
        ),
    ]);
    let grade = |input: &Submission| {
        assess(
            "round-1".into(),
            ("Challenger", "child"),
            std::slice::from_ref(&input.claim),
            std::slice::from_ref(input),
            &retained,
            vec![],
            true,
        )
    };
    assert!(grade(&input).unwrap().supported);
    let mut metadata = retained.clone();
    metadata.get_mut("r1-source1").unwrap()["object"]["access"] = json!("metadata");
    assert!(
        !assess(
            "round-1".into(),
            ("Challenger", "child"),
            std::slice::from_ref(&input.claim),
            std::slice::from_ref(&input),
            &metadata,
            vec![],
            true
        )
        .unwrap()
        .supported
    );
    input.inspections[0].evidence_id = "r1-memo".into();
    assert!(!grade(&input).unwrap().supported);
    input.inspections[0].evidence_id = "another-project".into();
    assert!(grade(&input).is_err());
    let mut forged = serde_json::to_value(&input).unwrap();
    forged["acceptedAt"] = json!(10);
    assert!(serde_json::from_value::<Submission>(forged).is_err());
}
