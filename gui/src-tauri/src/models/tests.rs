use super::*;
use chrono::NaiveDate;

// ── OrientationMap::empty ──────────────────────────────────────

fn empty_orientation() -> OrientationMap {
    OrientationMap {
        metadata: PaperMetadata {
            title: String::new(),
            authors: vec![],
            date: None,
            paper_type: PaperType::Mixed,
            page_count: None,
            has_appendix: false,
            has_online_appendix: false,
        },
        sections: vec![],
        formal_results: vec![],
        tables_figures: vec![],
        notation: vec![],
        stated_contribution: String::new(),
        key_references: vec![],
        extraction_quality_notes: vec![],
        review_plan: None,
    }
}

#[test]
fn old_step_telemetry_defaults_to_zero() {
    let output: StepOutput = serde_json::from_value(serde_json::json!({
        "step_id": "technical",
        "step_label": "Technical",
        "raw_text": "report",
        "calls": [{
            "role": "step",
            "input_tokens": 100,
            "output_tokens": 20
        }]
    }))
    .unwrap();

    assert_eq!(output.model_round_trips, 0);
    assert!(output.tool_calls.is_empty());
    assert_eq!(output.calls[0].model_round_trips, 0);
    assert!(output.calls[0].tool_calls.is_empty());
}

#[test]
fn tool_call_counts_sum_without_losing_unknowns() {
    let mut counts = ToolCallCounts {
        text_file: 2,
        unknown: 1,
        ..Default::default()
    };
    counts.add_counts(ToolCallCounts {
        image: 3,
        unknown: 4,
        ..Default::default()
    });

    assert_eq!(counts.total(), 10);
    assert_eq!(counts.text_file, 2);
    assert_eq!(counts.image, 3);
    assert_eq!(counts.unknown, 5);
}

#[test]
fn empty_detects_theory() {
    let text = "We prove Theorem 1. The proof proceeds by induction. Proposition 2 follows.";
    let omap = OrientationMap::empty(text);
    assert_eq!(omap.metadata.paper_type, PaperType::Theory);
}

#[test]
fn empty_detects_empirical() {
    let text = "Table 3 shows our regression results. The standard error is clustered. The coefficient is significant.";
    let omap = OrientationMap::empty(text);
    assert_eq!(omap.metadata.paper_type, PaperType::Empirical);
}

#[test]
fn empty_detects_mixed() {
    let text = "We prove Theorem 1 and then run a regression to estimate the coefficient.";
    let omap = OrientationMap::empty(text);
    assert_eq!(omap.metadata.paper_type, PaperType::Mixed);
}

#[test]
fn empty_defaults_to_mixed_for_short_text() {
    let omap = OrientationMap::empty("A short abstract.");
    assert_eq!(omap.metadata.paper_type, PaperType::Mixed);
}

// ── paper_view ─────────────────────────────────────────────────

#[test]
fn paper_view_accepts_paper_shaped_survey() {
    let survey = serde_json::json!({
        "metadata": {"title": "T", "paper_type": "theory"},
        "sections": [{"number": "1", "title": "Intro"}]
    });
    let view = paper_view(&survey).expect("paper-shaped survey should parse");
    assert_eq!(view.metadata.paper_type, PaperType::Theory);
    assert_eq!(view.sections.len(), 1);
}

#[test]
fn paper_view_accepts_serialized_empty_orientation() {
    let survey = serde_json::to_value(OrientationMap::empty("theorem proof")).unwrap();
    let view = paper_view(&survey).expect("placeholder orientation is paper-shaped");
    assert_eq!(view.metadata.paper_type, PaperType::Theory);
}

#[test]
fn paper_view_rejects_custom_survey() {
    let survey = serde_json::json!({
        "overview": "a codebase",
        "key_elements": [{"name": "main.rs"}]
    });
    assert!(paper_view(&survey).is_none());
}

#[test]
fn paper_view_rejects_null() {
    assert!(paper_view(&serde_json::Value::Null).is_none());
}

// ── survey_hint ────────────────────────────────────────────────

#[test]
fn survey_hint_paper_shaped_keeps_referee_vocabulary() {
    let survey = serde_json::json!({
        "metadata": {"title": "T", "paper_type": "theory"},
        "sections": []
    });
    assert!(survey_hint(&survey).contains("formal results"));
}

#[test]
fn survey_hint_generic_lists_actual_keys() {
    let survey = serde_json::json!({
        "overview": "a codebase",
        "components": [],
        "entry_points": []
    });
    let hint = survey_hint(&survey);
    assert!(hint.contains("overview"));
    assert!(hint.contains("components"));
    assert!(hint.contains("entry_points"));
    assert!(!hint.contains("paper"));
}

#[test]
fn survey_hint_empty_survey_stays_neutral() {
    let hint = survey_hint(&serde_json::json!({}));
    assert!(hint.contains("structured map"));
    assert!(!hint.contains("paper"));
}

// ── PipelineReport::all_outputs ───────────────────────────────────

fn make_report(
    step_outputs: Vec<StepOutput>,
    referees: Vec<RefereeReport>,
    editor: Option<EditorSynthesis>,
) -> PipelineReport {
    PipelineReport {
        orientation: serde_json::to_value(empty_orientation()).unwrap(),
        step_outputs,
        failed_steps: vec![],
        products: Default::default(),
        quality: Default::default(),
        referee_reports: referees,
        editor,
        report_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        paper_hash: "abc123".into(),
    }
}

#[test]
fn all_outputs_returns_step_outputs_when_present() {
    let outputs = vec![StepOutput {
        step_id: "s1".into(),
        step_label: "Step 1".into(),
        phase: "parallel".into(),
        agent: "claude".into(),
        raw_text: "content".into(),
        ..Default::default()
    }];
    let report = make_report(outputs.clone(), vec![], None);
    assert_eq!(report.all_outputs().len(), 1);
    assert_eq!(report.all_outputs()[0].step_id, "s1");
}

#[test]
fn all_outputs_converts_legacy_referees() {
    let referees = vec![RefereeReport {
        pass_name: "r1".into(),
        pass_label: "Referee 1".into(),
        agent: "claude".into(),
        steelman: String::new(),
        assessment: String::new(),
        raw_text: "referee content".into(),
    }];
    let report = make_report(vec![], referees, None);
    let outputs = report.all_outputs();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].step_id, "r1");
    assert_eq!(outputs[0].phase, "parallel");
}

#[test]
fn all_outputs_converts_legacy_editor() {
    let editor = EditorSynthesis {
        overall_assessment: "synthesis text".into(),
    };
    let report = make_report(vec![], vec![], Some(editor));
    let outputs = report.all_outputs();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].step_id, "editor_synthesis");
    assert_eq!(outputs[0].phase, "sequential");
    assert_eq!(outputs[0].raw_text, "synthesis text");
}

// ── PipelineReport::final_output ──────────────────────────────────

#[test]
fn final_output_prefers_sequential() {
    let outputs = vec![
        StepOutput {
            step_id: "s1".into(),
            step_label: "P1".into(),
            phase: "parallel".into(),
            agent: String::new(),
            raw_text: "parallel text".into(),
            ..Default::default()
        },
        StepOutput {
            step_id: "s2".into(),
            step_label: "Seq".into(),
            phase: "sequential".into(),
            agent: String::new(),
            raw_text: "sequential text".into(),
            ..Default::default()
        },
    ];
    let report = make_report(outputs, vec![], None);
    assert_eq!(report.final_output(), Some("sequential text"));
}

#[test]
fn final_output_honors_the_published_primary_step() {
    let outputs = vec![
        StepOutput {
            step_id: "published".into(),
            step_label: "Published".into(),
            phase: "sequential".into(),
            raw_text: "published text".into(),
            ..Default::default()
        },
        StepOutput {
            step_id: "later".into(),
            step_label: "Later".into(),
            phase: "sequential".into(),
            raw_text: "later text".into(),
            ..Default::default()
        },
    ];
    let mut report = make_report(outputs, vec![], None);
    report.products.schema_version = 1;
    report.products.primary_step_id = "published".into();
    assert_eq!(report.final_output(), Some("published text"));

    report.products.primary_step_id = "missing".into();
    assert_eq!(report.final_output(), None);
}

#[test]
fn final_output_falls_back_to_last() {
    let outputs = vec![StepOutput {
        step_id: "s1".into(),
        step_label: "P1".into(),
        phase: "parallel".into(),
        agent: String::new(),
        raw_text: "only parallel".into(),
        ..Default::default()
    }];
    let report = make_report(outputs, vec![], None);
    assert_eq!(report.final_output(), Some("only parallel"));
}

#[test]
fn final_output_does_not_promote_parallel_output_after_sequential_failure() {
    let outputs = vec![StepOutput {
        step_id: "technical".into(),
        step_label: "Technical".into(),
        phase: "parallel".into(),
        raw_text: "specialist analysis".into(),
        ..Default::default()
    }];
    let mut report = make_report(outputs, vec![], None);
    report.failed_steps.push(StepFailure {
        step_id: "synthesis".into(),
        step_label: "Synthesis".into(),
        phase: "sequential".into(),
        error: "invalid report".into(),
    });

    assert_eq!(report.final_output(), None);
    assert!(report.final_output_blocked_by_failed_sequential());
}

#[test]
fn final_output_ignores_blank_historical_outputs() {
    let outputs = vec![StepOutput {
        step_id: "technical".into(),
        step_label: "Technical".into(),
        phase: "parallel".into(),
        raw_text: "  \n".into(),
        ..Default::default()
    }];
    let report = make_report(outputs, vec![], None);
    assert_eq!(report.final_output(), None);
}

#[test]
fn final_output_legacy_editor() {
    let editor = EditorSynthesis {
        overall_assessment: "legacy synthesis".into(),
    };
    let report = make_report(vec![], vec![], Some(editor));
    assert_eq!(report.final_output(), Some("legacy synthesis"));
}

#[test]
fn final_output_empty_report() {
    let report = make_report(vec![], vec![], None);
    assert_eq!(report.final_output(), None);
}

// ── PaperType Display ──────────────────────────────────────────

#[test]
fn paper_type_display() {
    assert_eq!(PaperType::Theory.to_string(), "theory");
    assert_eq!(PaperType::Empirical.to_string(), "empirical");
    assert_eq!(PaperType::Mixed.to_string(), "mixed");
}
