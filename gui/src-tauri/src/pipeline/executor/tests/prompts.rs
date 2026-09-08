//! Prompts regression coverage.

use super::*;

#[test]
fn build_parallel_prompt_pdf() {
    let step = make_step("test", Phase::Parallel);
    let template = "{paper_type}\n{orientation}\n{step_prompt}\n{paper_path}\n{figure_hint}";
    let result = build_parallel_prompt(
        &step,
        "empirical",
        "/tmp/orient.json",
        "Read it for the paper's structure.",
        "/tmp/paper.txt",
        "",
        "/tmp/paper.pdf",
        template,
        "",
        None,
    )
    .unwrap();
    assert!(result.contains("empirical"));
    assert!(result.contains("/tmp/orient.json"));
    assert!(result.contains("Read it for the paper's structure."));
    assert!(result.contains("/tmp/paper.txt"));
    assert!(result.contains("original PDF"));
}

#[test]
fn build_parallel_prompt_recognizes_uppercase_pdf_extension() {
    let step = make_step("test", Phase::Parallel);
    let result = build_parallel_prompt(
        &step,
        "",
        "",
        "",
        "/tmp/paper.txt",
        "",
        "C:\\Papers\\DRAFT.PDF",
        "{figure_hint}",
        "",
        None,
    )
    .unwrap();
    assert!(result.contains("original PDF"));
}

#[test]
fn build_parallel_prompt_exposes_bundle_and_asset_root() {
    let step = make_step("test", Phase::Parallel);
    let result = build_parallel_prompt(
        &step,
        "empirical",
        "",
        "",
        "/tmp/document.md",
        "/tmp/document_bundle.json",
        "/tmp/paper.pdf",
        "{document_bundle}\n{figure_hint}",
        "",
        Some("/runs/r1/artifacts"),
    )
    .unwrap();
    assert!(result.contains("/tmp/document_bundle.json"));
    assert!(result.contains("/runs/r1"));
    assert!(result.contains("ReadDocumentAsset"));
}

#[test]
fn input_path_alias_substitutes_like_paper_path() {
    let step = make_step("test", Phase::Parallel);
    let template = "old={paper_path} new={input_path}";
    let result = build_parallel_prompt(
        &step,
        "empirical",
        "",
        "",
        "/tmp/paper.txt",
        "",
        "/tmp/p.pdf",
        template,
        "",
        None,
    )
    .unwrap();
    assert!(result.contains("old=/tmp/paper.txt"));
    assert!(result.contains("new=/tmp/paper.txt"));
}

#[test]
fn build_parallel_prompt_latex() {
    let step = make_step("test", Phase::Parallel);
    let template = "{figure_hint}";
    let result = build_parallel_prompt(
        &step,
        "theory",
        "",
        "",
        "/tmp/paper.txt",
        "",
        "/home/user/papers/main.tex",
        template,
        "",
        None,
    )
    .unwrap();
    assert!(result.contains("selected source context"));
    assert!(result.contains("bounded local dependencies"));
}

#[test]
fn build_parallel_prompt_empty_orientation() {
    let step = make_step("test", Phase::Parallel);
    let template = "[{orientation}]";
    let result = build_parallel_prompt(
        &step,
        "mixed",
        "",
        "unused hint",
        "/tmp/paper.txt",
        "",
        "/tmp/paper.pdf",
        template,
        "",
        None,
    )
    .unwrap();
    assert_eq!(result, "[]");
}

#[test]
fn build_parallel_prompt_output_format_substitution() {
    let step = make_step("test", Phase::Parallel);
    let template = "{step_prompt}\n{output_format}";
    let block = output_format_block(Some("/runs/r1/artifacts"), "testnonce", None);
    let result = build_parallel_prompt(
        &step,
        "",
        "",
        "",
        "/tmp/p.txt",
        "",
        "/tmp/p.pdf",
        template,
        &block,
        None,
    )
    .unwrap();
    assert!(result.contains("/runs/r1/artifacts/files/"));
    assert!(result.contains("response schema supplied by Pipeline"));
    assert!(!result.contains("JSON object"));
    assert!(!result.contains("PIPELINE REPORT testnonce START"));
    // Old templates without the placeholder receive the current contract.
    let old = "{step_prompt}\nREPORT START markers here";
    let result = build_parallel_prompt(
        &step,
        "",
        "",
        "",
        "/tmp/p.txt",
        "",
        "/tmp/p.pdf",
        old,
        &block,
        None,
    )
    .unwrap();
    assert!(!result.contains("{output_format}"));
    assert!(result.contains("REPORT START markers here"));
    assert!(result.contains("response schema supplied by Pipeline"));
}

#[test]
fn schema_output_format_defers_serialization_to_native_schema() {
    let schema = serde_json::json!({"type": "object", "properties": {}});
    let block = output_format_block(Some("/runs/r1/artifacts"), "unusednonce", Some(&schema));
    assert!(block.contains("response schema supplied by Pipeline"));
    assert!(block.contains("/runs/r1/artifacts/files/"));
    assert!(!block.contains("PIPELINE REPORT"));
    assert!(!block.contains("JSON"));
    assert!(!block.contains("markdown fences"));
}

#[test]
fn output_format_block_modes() {
    let write = output_format_block(Some("/runs/x/artifacts"), "nonce123", None);
    assert!(write.contains("/runs/x/artifacts/files/"));
    assert!(write.contains("response schema supplied by Pipeline"));
    assert!(!write.contains("PIPELINE REPORT nonce123 START"));
    let envelope = output_format_block(None, "nonce456", None);
    assert!(!envelope.contains("JSON object"));
    assert!(envelope.contains("Markdown report"));
    assert!(!envelope.contains("/runs/x/artifacts"));
}

#[test]
fn shared_context_note_names_only_selected_material() {
    let prompt =
        append_shared_context_note("Read the input at /tmp/paper.txt.".to_string(), true, false)
            .unwrap();
    assert!(prompt.contains("selected extracted input text"));
    assert!(!prompt.contains("survey"));
    assert!(prompt.contains("other artifacts listed"));
}

#[test]
fn evidence_retrieval_guidance_is_capability_aware_and_quality_preserving() {
    let text_only =
        append_evidence_retrieval_guidance("Task".into(), &["Read".to_string()]).unwrap();
    assert!(text_only.contains("ReadTextBatch"));
    assert!(!text_only.contains("ReadDocumentAssetsBatch"));
    assert!(!text_only.contains("web queries"));
    assert!(text_only.contains("continue sequentially"));
    assert!(text_only.contains("never omit evidence"));

    let all = append_evidence_retrieval_guidance(
        "Task".into(),
        &[
            "Read".to_string(),
            "ReadDocumentAsset".to_string(),
            "WebSearch".to_string(),
        ],
    )
    .unwrap();
    assert!(all.contains("ReadTextBatch"));
    assert!(all.contains("ReadDocumentAssetsBatch"));
    assert!(all.contains("complete set of independent web queries"));
    assert!(all.contains("Never substitute extracted text"));

    assert_eq!(
        append_evidence_retrieval_guidance("Task".into(), &[]).unwrap(),
        "Task"
    );
}

#[test]
fn tools_with_write_appends_once() {
    let base = vec!["Read".to_string()];
    assert_eq!(
        tools_with_write(&base, true, true, Some("/d")),
        vec!["Read", "ReadDocumentAsset", "Write"]
    );
    assert_eq!(
        tools_with_write(&base, true, true, None),
        vec!["Read", "ReadDocumentAsset"]
    );
    let with = vec!["Read".to_string(), "Write".to_string()];
    assert_eq!(
        tools_with_write(&with, false, false, Some("/d")),
        vec!["Write"]
    );
}
