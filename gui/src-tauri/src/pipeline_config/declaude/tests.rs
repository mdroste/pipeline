use super::*;
use crate::pipeline::structured;
use serde_json::json;

fn brief() -> serde_json::Value {
    json!({
        "document": {"type": "paper", "subject": "Wage setting", "audience": "Economists",
            "purpose": "Report estimates", "language": "English", "classification_evidence": "Abstract and methods sections"},
        "style_target": "Direct economic exposition", "style_anchors": [],
        "referees": [GENERAL_TOPIC], "selection_notes": "General topic editor: review available prose.",
        "sections": [], "protected_content": [], "extraction_quality_notes": []
    })
}

#[test]
fn declaude_profile_is_portable_and_publishes_only_verified_markdown() {
    let profile = declaude_profile();
    validate_profile_data(&profile).unwrap();
    assert!(profile_summary(DECLAUDE_ID.into(), &profile).builtin);
    assert_eq!(profile.outputs.primary_step, REPORT);
    assert!(profile.outputs.findings_step.is_empty());
    assert!(profile.steps.last().unwrap().output_schema.is_none());
    assert!(profile
        .steps
        .iter()
        .all(|step| step.enabled && step.tools.is_empty()));
    assert!(!profile.merge.enabled);
    let portable = WorkflowDocument::from_profile_data(profile.clone()).unwrap();
    let loaded = parse_workflow_document_strict(&portable.canonical_json).unwrap();
    assert_eq!(portable.fingerprint, loaded.fingerprint);
    assert_eq!(loaded.config.outputs, profile.outputs);
    assert_eq!(loaded.config.steps[0].system_prompt, CONTRACT);
    // Keep this workflow out of Paper Review's materialization and count policy.
    assert!(!crate::auto_review::uses_auto_review_contract(
        &loaded.config
    ));
    structured::provider_schema(profile.orientation_schema.as_ref().unwrap()).unwrap();
    structured::provider_schema(&schemas::edits_schema()).unwrap();
}

#[test]
fn declaude_orientation_enforces_one_to_three_unique_allowlisted_topics() {
    let schema = schemas::orientation_schema();
    let ids = [
        GENERAL_TOPIC,
        crate::auto_review::SUBJECTS[0].id,
        crate::auto_review::SUBJECTS[1].id,
    ];
    for count in 1..=3 {
        let mut value = brief();
        value["referees"] = json!(&ids[..count]);
        structured::validate(&schema, &value).unwrap();
        let profile = declaude_profile();
        let selected = profile.steps[..3]
            .iter()
            .filter(|step| {
                step.run_if.as_ref().is_none_or(|condition| {
                    crate::pipeline::conditions::condition_met(condition, &value, &[])
                })
            })
            .count();
        assert_eq!(selected, count);
    }
    for invalid in [
        json!([]),
        json!(["invented_role"]),
        json!([GENERAL_TOPIC, GENERAL_TOPIC]),
        json!(["a", "b", "c", "d"]),
    ] {
        let mut value = brief();
        value["referees"] = invalid;
        assert!(structured::validate(&schema, &value).is_err());
    }
    let prompt = orientation_prompt();
    for id in schema["properties"]["referees"]["items"]["enum"]
        .as_array()
        .unwrap()
    {
        assert!(prompt.contains(&format!("- {}:", id.as_str().unwrap())));
    }
    for kind in ["slides", "website", "mixed", "unknown"] {
        let mut value = brief();
        value["document"]["type"] = json!(kind);
        structured::validate(&schema, &value).unwrap();
    }
    let mut sampled = brief();
    sampled["extraction_quality_notes"] = json!([{
        "page_range": "global", "description": "Orientation used a section-aware sample."
    }]);
    structured::validate(&schema, &sampled).unwrap();
}

#[test]
fn declaude_edits_accept_clean_documents_and_require_exact_replacement_fields() {
    let schema = schemas::edits_schema();
    let mut value = json!({
        "coverage": {"reviewed": ["Section 1"], "limitations": [], "incomplete": false},
        "edits": [], "unresolved": []
    });
    structured::validate(&schema, &value).unwrap();
    value["edits"] = json!([{
        "location": "Section 1, second paragraph", "original": "Importantly, employment falls by 2%.",
        "replacement": "Employment falls by 2%.", "operation": "replace", "local_form": "slide bullet",
        "category": "signposting", "reason": "Removes prefatory emphasis.",
        "preserved_content": "Employment falls by 2%.", "context_support": [],
        "priority": "recommended", "confidence": "high", "referee_ids": [GENERAL_TOPIC]
    }]);
    structured::validate(&schema, &value).unwrap();
    value["edits"][0]
        .as_object_mut()
        .unwrap()
        .remove("replacement");
    assert!(structured::validate(&schema, &value).is_err());
    value["edits"] = json!([]);
    value["coverage"]["incomplete"] = json!(true);
    value["coverage"]["limitations"] = json!(["Selected second referee unavailable"]);
    structured::validate(&schema, &value).unwrap();
}

#[test]
fn declaude_dependencies_wait_for_all_slots_and_require_verification() {
    let profile = declaude_profile();
    let steps: Vec<_> = profile.steps.iter().collect();
    let dependencies = resolve_dependencies(&steps);
    for id in REFEREES {
        assert!(dependencies[3].contains(id));
        assert!(dependencies[4].contains(id));
    }
    assert!(dependencies[4].contains(EDITOR));
    assert_eq!(profile.steps[3].dependency_policy.required, [REFEREES[0]]);
    assert_eq!(
        profile.steps[4].dependency_policy.required,
        [REFEREES[0], EDITOR]
    );
    let plan = crate::pipeline::executor::execution_plan(&profile.into()).unwrap();
    assert_eq!(plan[2].step_ids, REFEREES);
    assert_eq!(plan[3].step_ids, [EDITOR]);
    assert_eq!(plan[4].step_ids, [REPORT]);
}

#[test]
fn declaude_install_preserves_customizations_and_reset_restores_stock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("declaude.json");
    let profile = declaude_profile();
    super::super::builtins::write_builtin_if_missing(&path, &profile).unwrap();
    let mut customized = profile.clone();
    customized.steps[0]
        .prompt
        .push_str("\nRetain British spelling.");
    save_profile_unlocked_at(&path, &customized).unwrap();
    super::super::builtins::write_builtin_if_missing(&path, &profile).unwrap();
    let saved: ProfileData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved.steps[0].prompt, customized.steps[0].prompt);
    super::super::profiles::reset_defaults_at(directory.path(), DECLAUDE_ID).unwrap();
    let reset: ProfileData = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(reset).unwrap(),
        serde_json::to_value(profile).unwrap()
    );
}

#[test]
fn declaude_markdown_is_primary_and_failed_verification_never_publishes_candidates() {
    let profile = declaude_profile();
    let markdown = "# Declaude review\n\n## Recommended edits\n\n**Original**\n> Importantly, employment falls by 2%.\n\n**Replacement**\n> Employment falls by 2%.";
    let mut report: crate::models::PipelineReport = serde_json::from_value(json!({
        "orientation": brief(), "products": {"primary_step_id": profile.outputs.primary_step},
        "step_outputs": [
            {"step_id": EDITOR, "step_label": "Candidates", "phase": "sequential", "raw_text": "unverified candidates", "structured_json": true},
            {"step_id": REPORT, "step_label": "Report", "phase": "sequential", "raw_text": markdown}
        ]
    })).unwrap();
    report.products = crate::findings::build_run_products(&profile.into(), &report.step_outputs);
    assert!(report.products.findings.is_none());
    assert_eq!(report.final_output(), Some(markdown));
    let rendered = crate::output::render_markdown(
        &report,
        None,
        std::time::Duration::ZERO,
        &crate::settings::Settings::default(),
    );
    assert!(rendered.contains(markdown));
    report.step_outputs.pop();
    report.failed_steps.push(crate::models::StepFailure {
        step_id: REPORT.into(),
        step_label: "Report".into(),
        phase: "sequential".into(),
        error: "Verification failed".into(),
    });
    assert!(report.final_output().is_none());
    let rendered = crate::output::render_markdown(
        &report,
        None,
        std::time::Duration::ZERO,
        &crate::settings::Settings::default(),
    );
    assert!(rendered.contains("Final report unavailable"));
}
