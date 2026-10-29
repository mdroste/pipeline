//! Analyzer steps in the portable workflow format.
use crate::pipeline_config::{parse_workflow_document_strict, StepKind};

fn workflow(check: serde_json::Value) -> String {
    serde_json::json!({
        "type": "profile",
        "schema_version": crate::pipeline_config::CURRENT_SCHEMA_VERSION,
        "name": "Reference check",
        "extraction": {"input_mode": "document"},
        "outputs": {"primary_step": "summary", "findings_step": "refs"},
        "steps": [
            check,
            {
                "id": "summary", "label": "Summary", "prompt": "Summarize {prior_outputs}",
                "enabled": true, "phase": "sequential",
                "context": {"include": [{"kind": "step", "step": "refs", "parts": ["report"]}]}
            }
        ]
    })
    .to_string()
}

fn analyzer_step(extra: serde_json::Value) -> serde_json::Value {
    let mut step = serde_json::json!({
        "id": "refs", "label": "Check References", "prompt": "", "enabled": true,
        "phase": "sequential", "kind": "analyzer",
        "analyzer": {"id": "reference_audit", "version": 1, "options": {"network": true}}
    });
    for (key, value) in extra.as_object().unwrap() {
        step[key] = value.clone();
    }
    step
}

#[test]
fn analyzer_workflows_import_and_keep_their_spec() {
    let document = parse_workflow_document_strict(&workflow(analyzer_step(serde_json::json!({}))))
        .expect("analyzer workflow imports");
    let step = &document.config.steps[0];
    assert_eq!(step.kind, StepKind::Analyzer);
    let spec = step.analyzer.as_ref().unwrap();
    assert_eq!((spec.id.as_str(), spec.version), ("reference_audit", 1));
    assert_eq!(spec.options["network"], true);
    assert!(document.canonical_json.contains("\"kind\": \"analyzer\""));
    // Model steps still serialize without the new fields.
    assert!(!document.canonical_json.contains("\"kind\": \"model\""));
}

#[test]
fn analyzer_steps_cannot_carry_model_call_fields() {
    for (extra, message) in [
        (
            serde_json::json!({"prompt": "Review this"}),
            "cannot have a prompt",
        ),
        (
            serde_json::json!({"agents": ["claude"]}),
            "cannot have providers",
        ),
        (
            serde_json::json!({"output_schema": {"type": "object"}}),
            "cannot have an output schema",
        ),
        (
            serde_json::json!({"analyzer": {"id": "reference_audit", "version": 9}}),
            "unknown analyzer",
        ),
        (
            serde_json::json!({"analyzer": {"id": "reference_audit", "version": 1, "options": {"shell": true}}}),
            "unknown option",
        ),
        (
            serde_json::json!({"analyzer": {"id": "reference_audit", "version": 1, "extra": 1}}),
            "extra",
        ),
        (
            serde_json::json!({"analyzer": null}),
            "must be a JSON object",
        ),
    ] {
        let error = parse_workflow_document_strict(&workflow(analyzer_step(extra.clone())))
            .expect_err(&format!("{extra} must be refused"));
        assert!(error.contains(message), "{extra}: {error}");
    }
}

#[test]
fn a_model_step_naming_an_analyzer_is_refused() {
    let step = serde_json::json!({
        "id": "refs", "label": "Refs", "prompt": "Check", "enabled": true, "phase": "sequential",
        "analyzer": {"id": "reference_audit", "version": 1}
    });
    let error = parse_workflow_document_strict(&workflow(step)).unwrap_err();
    assert!(error.contains("is not an analyzer step"), "{error}");
}

#[test]
fn analyzer_steps_need_no_provider() {
    let document =
        parse_workflow_document_strict(&workflow(analyzer_step(serde_json::json!({})))).unwrap();
    let mut config = document.config.clone();
    crate::pipeline_config::apply_agent_defaults(
        &mut config,
        &crate::settings::Settings::default(),
    );
    assert!(config.steps[0].agents.is_empty());
    assert!(!config.steps[1].agents.is_empty());
}
