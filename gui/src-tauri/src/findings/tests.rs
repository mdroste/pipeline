use super::legacy::parse_legacy_markdown_output;
use super::parse::parse_structured_output;
use super::*;
use crate::models::{Finding, FindingSet, PipelineReport, RunProducts, StepOutput};
use crate::pipeline_config::PipelineConfig;
use serde_json::Value;

fn output(text: &str) -> StepOutput {
    StepOutput {
        step_id: "final".into(),
        step_label: "Final".into(),
        phase: "sequential".into(),
        raw_text: text.into(),
        ..Default::default()
    }
}

#[test]
fn parses_existing_issues_and_source_evidence() {
    let parsed = parse_structured_output(&output(r#"{"issues":[{"id":"rule-a","title":"Broken invariant","severity":"major","section":"src/lib.rs","body":"Details","evidence":[{"source_path":"src/lib.rs","line_start":4,"line_end":7}]}]}"#)).unwrap();
    assert_eq!(parsed.findings[0].priority, "high");
    assert_eq!(parsed.findings[0].evidence[0].source_path, "src/lib.rs");
}

#[test]
fn paper_review_checks_legacy_document_paths_as_manuscript_evidence() {
    let mut config = crate::pipeline_config::workflow_template().unwrap().config;
    config.orientation_schema = Some(serde_json::json!({
        "x-pipeline-contract": crate::auto_review::AUTO_REVIEW_CONTRACT
    }));
    let mut step = output(
        r#"{"issues":[{"id":"result","title":"Result","evidence":[
            {"source_path":"document.md","page":1,"quote":"The estimate is positive."},
            {"source_path":"input/main/document.md","page":1,"quote":"The estimate is positive."},
            {"source_path":"document.md","line_start":1,"quote":"The estimate is positive."},
            {"evidence_type":"source","source_path":"document.md","page":1,"quote":"The estimate is positive."}
        ]}]}"#,
    );
    step.step_id = "review".into();
    config.outputs.findings_step = step.step_id.clone();
    let mut products = build_run_products(&config, &[step]);
    crate::pipeline::analyzers::quotes::verify_run(
        &mut products,
        None,
        &crate::models::ExtractionResult {
            text: "The estimate is positive.".into(),
            method: "text".into(),
            source_path: "paper.pdf".into(),
            paper_hash: "fixture".into(),
            quality_notes: Vec::new(),
        },
        None,
    );
    let findings = products.findings.unwrap();
    let evidence = &findings.findings[0].evidence;
    assert_eq!(evidence.len(), 4);
    for item in &evidence[..2] {
        assert!(item.source_path.is_empty());
        assert_eq!(item.page, Some(1));
        assert_eq!(item.verification_status, "verified");
    }
    for item in &evidence[2..] {
        assert_eq!(item.source_path, "document.md");
        assert_eq!(item.verification_status, "not_checkable");
    }
}

#[test]
fn parses_and_renders_stock_auto_markdown() {
    let parsed = parse_legacy_markdown_output(&output(
        "## Correctness and Internal Consistency\n\n**#1. Sign error in Proposition 2**\n\nThe proof changes sign.\n\n**#2. Missing support condition**\n\nThe claim is too broad.",
    )).unwrap();
    assert_eq!(parsed.findings.len(), 2);
    assert_eq!(
        parsed.findings[0].category,
        "Correctness and Internal Consistency"
    );
    assert!(render_markdown(&parsed).contains("**#2. Missing support condition**"));
}

#[test]
fn narrative_markdown_is_not_misclassified() {
    assert!(parse_legacy_markdown_output(&output("## Summary\n\nOrdinary prose.")).is_none());
}

#[test]
fn interleaved_categories_render_grouped_without_duplicate_headers() {
    let set = FindingSet {
        schema_version: FINDING_SCHEMA_VERSION,
        findings: vec![
            Finding {
                id: "sign".into(),
                title: "Sign error".into(),
                category: CATEGORIES[0].into(),
                body: "A".into(),
                ..Default::default()
            },
            Finding {
                id: "power".into(),
                title: "Underpowered design".into(),
                category: CATEGORIES[1].into(),
                body: "B".into(),
                ..Default::default()
            },
            Finding {
                id: "ref".into(),
                title: "Broken cross-reference".into(),
                category: CATEGORIES[0].into(),
                body: "C".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let markdown = render_markdown(&set);
    // One header per category, first-appearance order, renumbered in
    // rendered order.
    assert_eq!(markdown.matches("## Correctness").count(), 1);
    assert!(markdown.find("## Correctness").unwrap() < markdown.find("## Methodological").unwrap());
    assert!(markdown.contains("**#2. Broken cross-reference**"));
    assert!(markdown.contains("**#3. Underpowered design**"));
}

#[test]
fn sources_are_parsed_and_bounded() {
    let parsed = parse_structured_output(&output(
        r#"{"findings":[{"id":"a","title":"T","category":"Contribution and Scope","body":"B","sources":["auto_exposition","economics_econometrics"],"evidence":[]}]}"#,
    ))
    .unwrap();
    assert_eq!(
        parsed.findings[0].sources,
        vec!["auto_exposition", "economics_econometrics"]
    );
}

#[test]
fn legacy_products_do_not_promote_structured_specialist_reports() {
    let mut report = PipelineReport {
        orientation: Value::Null,
        step_outputs: vec![output(
            r#"{"findings":[{"title":"T","in_the_paper":"Claim","problem":"Problem","consequence":"Consequence","what_would_help":"Fix","evidence":[]}]}"#,
        )],
        failed_steps: Vec::new(),
        products: RunProducts::default(),
        quality: Default::default(),
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        paper_hash: String::new(),
    };
    report.step_outputs[0].phase = "parallel".into();
    report.step_outputs[0].structured_json = true;

    ensure_legacy_products(&mut report);

    assert!(report.products.findings.is_none());
}

#[test]
fn canonical_schema_is_portable_and_matches_its_own_product() {
    let schema = output_schema();
    crate::pipeline::structured::provider_schema(&schema).unwrap();
    let product = serde_json::json!({
      "schema_version": 2, "taxonomy": CATEGORIES,
      "findings": [{
        "rank": 1, "id": "sign-error", "title": "Sign error",
        "category": CATEGORIES[0], "severity": "high", "confidence": "high",
        "verification_status": "unverified", "reviewer_ids": ["auto_consistency"],
        "problem": "The sign is reversed.", "consequence": "The proposition fails.",
        "recommended_action": "Correct the derivation.",
        "evidence": [{"evidence_type": "document", "verification_status": "unverified", "page": 4, "description": "Proposition 2", "quote": "q"}]
    }]});
    crate::pipeline::structured::validate(&schema, &product).unwrap();
    // Unknown categories and empty ids are rejected by the contract.
    let bad_category = serde_json::json!({"findings": [{
        "id": "x", "title": "T", "category": "Novel Category",
        "body": "B", "evidence": []
    }]});
    assert!(crate::pipeline::structured::validate(&schema, &bad_category).is_err());
    let empty_id = serde_json::json!({"findings": [{
        "id": "", "title": "T", "category": CATEGORIES[0],
        "body": "B", "evidence": []
    }]});
    assert!(crate::pipeline::structured::validate(&schema, &empty_id).is_err());
}

#[test]
fn empty_structured_findings_are_a_valid_product() {
    let parsed = parse_structured_output(&output(r#"{"findings":[]}"#)).unwrap();
    assert!(parsed.findings.is_empty());
    assert_eq!(render_markdown(&parsed), "No findings.");
}

#[test]
fn an_explicit_findings_step_never_falls_back_to_another_step() {
    let mut earlier = output(r#"{"issues":[{"id":"old","title":"Old","body":"Old"}]}"#);
    earlier.step_id = "earlier".into();
    let mut declared = output("not structured");
    declared.step_id = "declared".into();
    let mut config = PipelineConfig {
        steps: Vec::new(),
        merge: Default::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };
    config.outputs.primary_step = "declared".into();
    config.outputs.findings_step = "declared".into();

    let products = build_run_products(&config, &[earlier, declared]);
    assert_eq!(products.primary_step_id, "declared");
    assert!(products.findings.is_none());
}
