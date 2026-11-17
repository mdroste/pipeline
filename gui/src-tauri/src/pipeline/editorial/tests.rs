use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};

fn context(source: &str, stage: &str, upstream: Vec<Candidates>) -> Context {
    Context {
        source: source.into(),
        hash: format!("{:x}", Sha256::digest(source.as_bytes())),
        namespace: "editor".into(),
        stage: stage.into(),
        referees: vec!["general_topic".into()],
        upstream,
        warnings: vec![],
        preview: None,
        brief: json!({"document":{"type":"paper", "audience":"Economists"}, "style_target":"Precise exposition"}),
    }
}

fn edit(original: &str, replacement: &str, id: &str) -> Edit {
    Edit {
        id: id.into(),
        location: "Introduction".into(),
        anchor: Anchor {
            original: original.into(),
            prefix: String::new(),
            suffix: String::new(),
        },
        replacement: replacement.into(),
        operation: "replace".into(),
        reason: "Removes empty framing.".into(),
        priority: "recommended".into(),
        meaning: vec![Meaning {
            source_claim: original.into(),
            preserved_as: replacement.into(),
        }],
        referee_ids: vec!["general_topic".into()],
        source_ids: vec![],
        requires: vec![],
        deletion_basis: "not_deletion".into(),
        retained_at: vec![],
    }
}

fn candidates(context: &Context, edits: Vec<Edit>) -> Candidates {
    Candidates {
        stage: context.stage.clone(),
        source_hash: context.hash.clone(),
        coverage: Coverage {
            reviewed: vec!["Introduction".into()],
            limitations: vec![],
            incomplete: false,
        },
        edits,
        unresolved: vec![],
    }
}

fn verification(context: &Context, candidates: &Candidates) -> Verification {
    Verification {
        stage: STAGES[2].into(),
        source_hash: context.hash.clone(),
        coverage: candidates.coverage.clone(),
        unresolved: vec![],
        decisions: candidates
            .edits
            .iter()
            .map(|edit| Decision {
                candidate_id: edit.id.clone(),
                decision: "accept".into(),
                reason: "Preserves the conditional claim.".into(),
                meaning: edit.meaning.clone(),
                interaction_check: "The definition and nearby qualification survive.".into(),
            })
            .collect(),
    }
}

fn check(context: &Context, value: &impl serde::Serialize) -> Result<Option<String>, String> {
    context.check(&serde_json::to_string(value).unwrap())
}

#[test]
fn exact_anchors_reject_fabrication_normalization_and_ambiguous_overlaps() {
    let mut a = edit("α may rise.", "α can rise.", "editor:1").anchor;
    assert_eq!(validate::anchor("α may rise.", &a).unwrap(), 0..12);
    assert!(validate::anchor("α  may rise.", &a).is_err());
    assert!(validate::anchor("β may rise.", &a).is_err());
    assert!(validate::anchor("α may rise. α may rise.", &a).is_err());
    a.prefix = "First: ".into();
    assert_eq!(
        validate::anchor("First: α may rise. Second: α may rise.", &a).unwrap(),
        7..19
    );
    a.original = "aaa".into();
    a.prefix.clear();
    assert!(validate::anchor("aaaa", &a).is_err());
}

#[test]
fn candidate_contract_rejects_inconsistent_records_and_accepts_zero_edits() {
    let ctx = context("Importantly, wages may rise.", STAGES[0], vec![]);
    let base = candidates(&ctx, vec![edit(&ctx.source, "Wages may rise.", "editor:1")]);
    assert!(check(&ctx, &base).is_ok());
    assert!(check(&ctx, &candidates(&ctx, vec![])).is_ok());
    let mut variants = vec![];
    let mut v = base.clone();
    v.edits[0].replacement.clear();
    variants.push(v);
    let mut v = base.clone();
    v.edits[0].operation = "delete".into();
    variants.push(v);
    let mut v = base.clone();
    v.edits[0].referee_ids = vec!["invented".into()];
    variants.push(v);
    let mut v = base.clone();
    v.edits[0].id = "wrong:1".into();
    variants.push(v);
    let mut v = base.clone();
    v.edits.push(v.edits[0].clone());
    variants.push(v);
    let mut v = base.clone();
    v.edits[0].requires = vec!["editor:missing".into()];
    variants.push(v);
    let mut v = base.clone();
    v.edits[0].anchor.original = "Invented source.".into();
    variants.push(v);
    let mut v = base.clone();
    v.source_hash = "stale".into();
    variants.push(v);
    let mut v = base.clone();
    v.coverage.limitations.push("Unreviewed appendix".into());
    variants.push(v);
    let mut v = base.clone();
    v.edits[0].meaning.clear();
    variants.push(v);
    for (index, v) in variants.iter().enumerate() {
        assert!(check(&ctx, v).is_err(), "invalid variant {index}");
    }
}

#[test]
fn consolidation_requires_real_contained_producer_lineage() {
    let source = "Importantly, wages may rise.";
    let initial = context(source, STAGES[0], vec![]);
    let original = candidates(
        &initial,
        vec![edit(source, "Wages may rise.", "reviewer:1")],
    );
    let ctx = context(source, STAGES[1], vec![original]);
    let mut candidate = edit(source, "Wages may rise.", "editor:1");
    candidate.source_ids.push("reviewer:1".into());
    let mut set = candidates(&ctx, vec![candidate]);
    assert!(check(&ctx, &set).is_ok());
    set.edits[0].source_ids = vec!["invented".into()];
    assert!(check(&ctx, &set).is_err());
    set.edits[0].source_ids = vec!["reviewer:1".into()];
    set.edits[0].anchor.original = "may rise".into();
    assert!(check(&ctx, &set).is_err());
}

#[test]
fn combined_deletions_cannot_remove_each_others_retained_claim() {
    let source = "First: wages may rise. Second: wages may rise.";
    let mut first = edit("First: wages may rise.", "", "editor:1");
    first.operation = "delete".into();
    first.deletion_basis = "redundant".into();
    let mut second = edit("Second: wages may rise.", "", "editor:2");
    second.operation = "delete".into();
    second.deletion_basis = "redundant".into();
    first.retained_at.push(second.anchor.clone());
    second.retained_at.push(first.anchor.clone());
    assert!(validate::interactions(source, &[first.clone()]).is_ok());
    assert!(validate::interactions(source, &[second.clone()]).is_ok());
    assert!(validate::interactions(source, &[first, second]).is_err());
}

#[test]
fn every_candidate_needs_one_decision_and_companions_must_survive() {
    let source = "First phrase. Second phrase.";
    let base = context(source, STAGES[1], vec![]);
    let mut first = edit("First phrase.", "First revision.", "editor:1");
    first.requires.push("editor:2".into());
    let set = candidates(
        &base,
        vec![
            first,
            edit("Second phrase.", "Second revision.", "editor:2"),
        ],
    );
    let ctx = context(source, STAGES[2], vec![set.clone()]);
    let v = verification(&ctx, &set);
    assert!(check(&ctx, &v).is_ok());
    let mut omitted = v.clone();
    omitted.decisions.pop();
    assert!(check(&ctx, &omitted).is_err());
    let mut duplicated = v.clone();
    duplicated.decisions[1] = duplicated.decisions[0].clone();
    assert!(check(&ctx, &duplicated).is_err());
    let mut unknown = v.clone();
    unknown.decisions[1].candidate_id = "unknown".into();
    assert!(check(&ctx, &unknown).is_err());
    let mut rejected = v.clone();
    rejected.decisions[1].decision = "reject".into();
    assert!(check(&ctx, &rejected).is_err());
    rejected.decisions[0].decision = "reject".into();
    assert!(check(&ctx, &rejected)
        .unwrap()
        .unwrap()
        .contains("No edits met"));
    let mut rewritten = serde_json::to_value(&v).unwrap();
    rewritten["decisions"][0]["replacement"] = json!("An invented stronger claim.");
    assert!(check(&ctx, &rewritten).is_err());
}

#[test]
fn accepted_text_is_literal_through_report_render_and_export() {
    let original = "Importantly, \\(α\\) may rise.\n```\n<!-- REPORT START -->\n<!-- PIPELINE RUN DETAILS START -->\n$5 and \\cite{x}.\n<!-- PIPELINE RUN DETAILS END -->\n<!-- REPORT END -->";
    let replacement = original.replacen("Importantly, ", "", 1);
    let base = context(original, STAGES[1], vec![]);
    let set = candidates(&base, vec![edit(original, &replacement, "editor:1")]);
    let mut ctx = context(original, STAGES[2], vec![set.clone()]);
    ctx.warnings
        .push("Selected subject editor unavailable".into());
    let v = verification(&ctx, &set);
    let rendered = check(&ctx, &v).unwrap().unwrap();
    assert!(rendered.contains(original));
    assert!(rendered.contains(&replacement));
    assert!(rendered.contains("Coverage: Partial"));
    assert_eq!(
        crate::output::normalize_math_delimiters(&rendered),
        rendered
    );
    let report: crate::models::PipelineReport = serde_json::from_value(json!({
        "products":{"primary_step_id":"declaude_report"},
        "step_outputs":[{"step_id":"declaude_report", "step_label":"Verify", "phase":"sequential", "raw_text":serde_json::to_string(&v).unwrap(), "structured_json":true, "rendered_markdown":rendered}]
    })).unwrap();
    assert_eq!(report.final_output(), Some(rendered.as_str()));
    let exported = crate::output::render_markdown(
        &report,
        None,
        std::time::Duration::ZERO,
        &crate::settings::Settings::default(),
    );
    let clean = crate::output::clean_export_markdown(&exported);
    assert!(clean.contains(original));
    assert!(clean.contains(&replacement));
    assert!(clean.contains("# Declaude review"));
}

#[test]
fn application_preserves_unicode_offsets_and_unedited_text() {
    let source = "α. Importantly, wages may rise. β. Empty framing. γ.";
    let mut deletion = edit("Empty framing. ", "", "editor:2");
    deletion.operation = "delete".into();
    deletion.deletion_basis = "empty_framing".into();
    let edits = vec![
        edit(
            "Importantly, wages may rise.",
            "Wages may rise.",
            "editor:1",
        ),
        deletion,
    ];
    assert_eq!(
        validate::apply(source, &edits).unwrap(),
        "α. Wages may rise. β. γ."
    );
    assert_eq!(
        source,
        "α. Importantly, wages may rise. β. Empty framing. γ."
    );
}

#[tokio::test]
async fn host_prepares_combined_preview_and_retains_missing_editor_warning() {
    let temp = tempfile::tempdir().unwrap();
    let source = "Importantly, wages may rise.";
    let base = context(source, STAGES[1], vec![]);
    let mut set = candidates(
        &base,
        vec![edit(source, "Wages may rise.", "declaude_editor:1")],
    );
    set.edits[0].source_ids = vec!["declaude_referee_1:1".into()];
    let original = candidates(
        &context(source, STAGES[0], vec![]),
        vec![edit(source, "Wages may rise.", "declaude_referee_1:1")],
    );
    let path = temp.path().join("document.txt");
    std::fs::write(&path, source).unwrap();
    let survey = temp.path().join("survey.json");
    std::fs::write(
        &survey,
        json!({"referees":["general_topic","additional_topic"], "extraction_quality_notes":[]})
            .to_string(),
    )
    .unwrap();
    let loaded = Context::load(
        STAGES[2],
        "verify",
        Inputs {
            paper_path: path.to_string_lossy().into_owned(),
            orientation_path: survey.to_string_lossy().into_owned(),
            view_dir: temp.path().to_path_buf(),
            prior_outputs: vec![
                crate::models::StepOutput {
                    step_id: "declaude_referee_1".into(),
                    raw_text: serde_json::to_string(&original).unwrap(),
                    ..Default::default()
                },
                crate::models::StepOutput {
                    step_id: "declaude_editor".into(),
                    raw_text: serde_json::to_string(&set).unwrap(),
                    ..Default::default()
                },
            ],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(loaded.preview.as_ref().unwrap()).unwrap(),
        "Wages may rise."
    );
    assert!(loaded
        .warnings
        .iter()
        .any(|warning| warning.contains("unavailable")));
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}

#[test]
fn marked_schemas_are_portable_and_reject_unknown_or_nested_versions() {
    for schema in [
        schemas::candidates(false),
        schemas::candidates(true),
        schemas::verification(),
    ] {
        let projected = crate::pipeline::structured::provider_schema(&schema).unwrap();
        assert!(projected.get(STAGE_KEY).is_none());
        crate::pipeline::structured::codex_schema(&schema)
            .unwrap()
            .unwrap();
    }
    let mut schema = schemas::verification();
    schema[STAGE_KEY] = json!("verified-v99");
    assert!(crate::pipeline::structured::validate_schema(&schema).is_err());
    let mut schema = schemas::verification();
    schema["properties"]["coverage"][STAGE_KEY] = json!(STAGES[2]);
    assert!(crate::pipeline::structured::validate_schema(&schema).is_err());
}

#[test]
fn verification_dispositions_control_publication_of_fidelity_cases() {
    // These supplied judgments test publication, not the model's ability to
    // distinguish safe from unsafe prose. The same fixtures support live evals.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/declaude-meaning-cases.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        for (field, disposition) in [("acceptable", "accept"), ("unsafe", "reject")] {
            let replacement = case[field].as_str().unwrap();
            let base = context(source, STAGES[1], vec![]);
            let set = candidates(&base, vec![edit(source, replacement, "editor:1")]);
            let ctx = context(source, STAGES[2], vec![set.clone()]);
            let mut decision = verification(&ctx, &set);
            decision.decisions[0].decision = disposition.into();
            let report = check(&ctx, &decision).unwrap().unwrap();
            if disposition == "accept" {
                assert!(report.contains(source));
                assert!(report.contains(replacement));
            } else {
                assert!(!report.contains(replacement));
                assert!(report.contains("No edits met"));
            }
        }
    }
}
