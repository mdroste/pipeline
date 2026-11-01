//! The designer's step edits, checked against the real validators.
//!
//! The designer edits a workflow in TypeScript and only Rust validates it, so a
//! shared fixture joins the two. The designer's test turns each `source` into
//! its result through `useStepActions`. Each result must then pass every
//! validator a save runs, and the state the designer steers around must fail.

use super::*;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../../../tests/fixtures/workflow/step-editing.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn profile(graph: &Value) -> ProfileData {
    serde_json::from_value(json!({
        "name": "Step editing",
        "steps": graph["steps"],
        "outputs": graph["outputs"],
    }))
    .unwrap()
}

fn save_result(profile: &ProfileData) -> Result<(), String> {
    validate_dependencies(&profile.steps)?;
    validate_profile_data(profile)?;
    validate_enabled_sequential_step(&profile.steps)
}

fn step_mut<'a>(graph: &'a mut Value, id: &str) -> &'a mut Value {
    graph["steps"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|step| step["id"] == id)
        .unwrap()
}

#[test]
fn the_fixture_uses_the_save_limits() {
    let fixture = fixture();
    assert_eq!(fixture["limits"]["step_label_chars"], MAX_STEP_LABEL_CHARS);
    let limit = fixture["limits"]["step_id_chars"].as_u64().unwrap() as usize;
    for (id, accepted) in [("x".repeat(limit), true), ("x".repeat(limit + 1), false)] {
        let step = StepConfig {
            id,
            ..Default::default()
        };
        assert_eq!(validate_unique_step_ids(&[step]).is_ok(), accepted);
    }
}

#[test]
fn a_check_publishes_findings_without_a_schema() {
    let fixture = fixture();
    let role = &fixture["findings_role"];
    let step = role["step"].as_str().unwrap();
    save_result(&profile(&role["ticked"])).unwrap();
    save_result(&profile(&role["unticked"])).unwrap();

    // Adding a schema, as ticking the role once did, is what a save refuses.
    let mut with_schema = role["ticked"].clone();
    step_mut(&mut with_schema, step)["output_schema"] = json!({ "type": "object" });
    let error = save_result(&profile(&with_schema)).unwrap_err();
    assert!(error.contains("cannot have an output schema"), "{error}");
}

#[test]
fn a_step_switched_to_sequential_saves() {
    let fixture = fixture();
    for case in fixture["sequential"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        save_result(&profile(&case["result"])).unwrap_or_else(|error| panic!("{name}: {error}"));

        // Only the cases the designer asks about would otherwise be refused.
        let mut phase_only = case["source"].clone();
        step_mut(&mut phase_only, case["step"].as_str().unwrap())["phase"] = json!("sequential");
        assert_eq!(
            save_result(&profile(&phase_only)).is_err(),
            case["confirms"].as_bool().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn a_long_step_name_produces_an_id_that_saves() {
    let fixture = fixture();
    let long = &fixture["long_label"];
    save_result(&profile(&long["result"])).unwrap();

    let mut overlong = long["result"].clone();
    overlong["steps"][0]["id"] = long["overlong_id"].clone();
    let error = save_result(&profile(&overlong)).unwrap_err();
    assert!(error.contains("Invalid step id"), "{error}");
}

#[test]
fn ids_made_unique_stay_valid() {
    let fixture = fixture();
    for case in fixture["step_ids"].as_array().unwrap() {
        let mut ids: Vec<&str> = case["taken"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap())
            .collect();
        ids.push(case["id"].as_str().unwrap());
        let steps: Vec<StepConfig> = ids
            .into_iter()
            .map(|id| StepConfig {
                id: id.into(),
                ..Default::default()
            })
            .collect();
        validate_unique_step_ids(&steps).unwrap_or_else(|error| panic!("{case}: {error}"));
    }
}
