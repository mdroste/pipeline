//! The published findings the Issues tab reads, as the backend really writes
//! them. The frontend's tests read `published` from this same fixture, so a
//! test double cannot hide a field that a real findings-v2 review never sets.

use pipeline_gui_lib::findings::{output_schema, parse_value, validate_v2_semantics};
use serde_json::Value;

const FIXTURE: &str = include_str!("../../tests/fixtures/workflow/findings-v2.json");

#[test]
fn a_findings_v2_artifact_publishes_the_fixture_findings() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let artifact = &fixture["artifact"];
    validate_v2_semantics(&output_schema(), artifact).unwrap();
    let published = parse_value(
        artifact,
        fixture["source_step"]["id"].as_str().unwrap(),
        fixture["source_step"]["label"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&published).unwrap(),
        fixture["published"]
    );
    assert!(published
        .findings
        .iter()
        .all(|finding| finding.sources.is_empty()));
}
