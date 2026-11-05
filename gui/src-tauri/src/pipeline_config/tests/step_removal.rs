//! The designer's step removal, checked against the real validators.
//!
//! The designer edits a workflow in TypeScript and only Rust validates it, so a
//! shared fixture joins the two. `source` must equal the real profile here. The
//! designer's test turns `source` into each `disabled` and `removed` result
//! through `useStepActions`. Each result, applied to the real profile, must then
//! pass every validator a save runs.

use super::*;
use crate::pipeline::structured::PRESERVE_FINDINGS_KEY;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../../../tests/fixtures/workflow/step-removal.json");
const GRAPH_FIELDS: [&str; 7] = [
    "id",
    "enabled",
    "after",
    "context",
    "run_if",
    "dependency_policy",
    "for_each",
];

fn graph(profile: &ProfileData) -> Value {
    let steps = profile.steps.iter().map(|step| {
        let value = serde_json::to_value(step).unwrap();
        let mut fields: serde_json::Map<_, _> = GRAPH_FIELDS
            .iter()
            .filter_map(|key| Some((key.to_string(), value.get(key)?.clone())))
            .collect();
        if let Some(target) = step
            .output_schema
            .as_ref()
            .and_then(|schema| schema.get(PRESERVE_FINDINGS_KEY))
        {
            fields.insert(
                "output_schema".into(),
                json!({ PRESERVE_FINDINGS_KEY: target }),
            );
        }
        Value::Object(fields)
    });
    json!({ "steps": steps.collect::<Vec<_>>(), "outputs": profile.outputs })
}

/// Overlay an edited graph on the real profile, keeping its prompts and schemas.
fn with_graph(mut profile: ProfileData, edited: &Value) -> ProfileData {
    let edits = edited["steps"].as_array().unwrap();
    let edit_of = |id: &str| edits.iter().find(|edit| edit["id"] == id);
    profile.steps.retain(|step| edit_of(&step.id).is_some());
    for step in &mut profile.steps {
        let edit = edit_of(&step.id).unwrap();
        let mut value = serde_json::to_value(&*step).unwrap();
        for key in GRAPH_FIELDS {
            match edit.get(key) {
                Some(field) => value[key] = field.clone(),
                None => {
                    value.as_object_mut().unwrap().remove(key);
                }
            }
        }
        if edit["output_schema"].get(PRESERVE_FINDINGS_KEY).is_none() {
            if let Some(schema) = value["output_schema"].as_object_mut() {
                schema.remove(PRESERVE_FINDINGS_KEY);
            }
        }
        *step = serde_json::from_value(value).unwrap();
    }
    profile.outputs = serde_json::from_value(edited["outputs"].clone()).unwrap();
    profile
}

fn assert_saves(profile: &ProfileData, label: &str) {
    validate_dependencies(&profile.steps).unwrap_or_else(|error| panic!("{label}: {error}"));
    validate_profile_data(profile).unwrap_or_else(|error| panic!("{label}: {error}"));
    validate_enabled_sequential_step(&profile.steps)
        .unwrap_or_else(|error| panic!("{label}: {error}"));
}

/// Quorum and named outputs do not appear in a built-in, so this covers them.
fn quorum_profile() -> ProfileData {
    let parallel = |id: &str| StepConfig {
        id: id.into(),
        label: id.into(),
        phase: Phase::Parallel,
        enabled: true,
        ..Default::default()
    };
    let mut report = StepConfig {
        id: "report".into(),
        label: "Report".into(),
        phase: Phase::Sequential,
        enabled: true,
        ..Default::default()
    };
    report.context.include.push(ArtifactSelector::Step {
        step: "first".into(),
        parts: vec![StepArtifactPart::Report],
        glob: String::new(),
    });
    report.dependency_policy = DependencyPolicy {
        required: vec!["first".into()],
        quorum: vec!["second".into(), "third".into()],
        minimum_successes: 2,
    };
    let mut profile = ProfileData::new(
        "Quorum",
        vec![
            parallel("first"),
            parallel("second"),
            parallel("third"),
            report,
        ],
        MergeConfig::default(),
    );
    profile.outputs.primary_step = "report".into();
    for (key, step) in [("second_notes", "second"), ("third_notes", "third")] {
        profile.outputs.named.push(NamedProductSpec {
            key: key.into(),
            step: step.into(),
            ..Default::default()
        });
    }
    profile
}

#[test]
fn step_removal_results_from_the_designer_pass_every_save_validator() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    for (name, build) in [
        (
            "grant-review",
            super::super::builtins::grant_review_profile as fn() -> ProfileData,
        ),
        ("declaude", super::super::declaude::declaude_profile),
        ("quorum", quorum_profile),
    ] {
        let entry = &fixture[name];
        assert_eq!(
            graph(&build()),
            entry["source"],
            "{name}: the fixture no longer matches this profile's dependency graph"
        );
        for case in entry["cases"].as_array().unwrap() {
            let step = case["step"].as_str().unwrap();
            for result in ["disabled", "removed"] {
                let edited = with_graph(build(), &case[result]);
                assert_saves(&edited, &format!("{name}: {result} {step}"));
            }

            // Turning the step off without the designer's clean-up is the
            // failure researchers met, so the fixture's edits are what pass.
            let mut untouched = build();
            untouched
                .steps
                .iter_mut()
                .filter(|candidate| candidate.id == step)
                .for_each(|candidate| candidate.enabled = false);
            let refused = validate_dependencies(&untouched.steps).is_err()
                || validate_profile_data(&untouched).is_err();
            assert!(refused, "{name}: disabling {step} alone should not save");
        }
    }
}
