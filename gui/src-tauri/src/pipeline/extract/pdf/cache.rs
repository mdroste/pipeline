//! Existing PDF extraction cache identities and active-parser bookkeeping.

use super::super::core::{PADDLE_FULL_ACTIVE_FILE, PADDLE_STRUCTURE_SCHEMA};
use super::super::structured::{atomic_write_cache, cache_fingerprint, EXTRACTION_CACHE_SCHEMA};
use super::llm::{
    provider_uses_direct_api, EXTRACTION_MAX_OUTPUT_TOKENS, LLM_CHUNK_MAX_PAGES,
    LLM_CHUNK_TARGET_BASELINE_CHARS,
};
use std::path::{Path, PathBuf};

pub(in crate::pipeline::extract) fn llm_extraction_cache_path(
    hash: &str,
    settings: &crate::settings::Settings,
) -> Option<PathBuf> {
    if hash.len() != 16 || !hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return None;
    }
    let identity = serde_json::json!({
        "schema": EXTRACTION_CACHE_SCHEMA,
        "app_version": env!("CARGO_PKG_VERSION"),
        "engine": "llm-pdf-bounded-v2",
        "provider": settings.preferred_provider,
        "transport": if provider_uses_direct_api(settings) { "api" } else { "cli" },
        "model_policy": match settings.default_model.as_str() {
            "" => "automatic",
            model => model,
        },
        "effort_policy": "provider_default",
        "chunk_pages": LLM_CHUNK_MAX_PAGES,
        "chunk_chars": LLM_CHUNK_TARGET_BASELINE_CHARS,
        "max_output_tokens": EXTRACTION_MAX_OUTPUT_TOKENS,
    });
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("llm-pdf")
            .join(hash)
            .join(format!("{}.md", cache_fingerprint(&identity))),
    )
}

pub(in crate::pipeline::extract) fn full_parser_cache_fingerprint(
    paths: &crate::engines::PaddleFullParserPaths,
    settings: &crate::settings::Settings,
) -> String {
    cache_fingerprint(&serde_json::json!({
        "schema": EXTRACTION_CACHE_SCHEMA,
        "structure_schema": PADDLE_STRUCTURE_SCHEMA,
        "engine": "paddleocr-vl-full",
        "parser_release": paths.release,
        "parser_integrity": paths.integrity_sha256,
        "sidecar_sha256": paths.sidecar_sha256,
        "recognition_integrity": match &paths.recognition {
            crate::engines::PaddleRecognition::Llama(engine) => engine.integrity_sha256.as_str(),
            // The parser's own integrity digest binds built-in weights.
            crate::engines::PaddleRecognition::BuiltIn { .. } => "built-in",
        },
        "concurrency": crate::settings::resolved_paddle_page_concurrency(settings),
        "vision_batch": crate::settings::resolved_paddle_mtmd_batch_tokens(settings),
        "flash_attention": settings.paddle_flash_attention,
        "max_output_tokens": settings.paddle_max_output_tokens,
        "page_retries": settings.paddle_page_retries,
        "layout_detection": settings.paddle_full_layout_detection,
        "layout_threshold": settings.paddle_full_layout_threshold,
        "layout_nms": settings.paddle_full_layout_nms,
        "layout_merge_bboxes_mode": settings.paddle_full_layout_merge_bboxes_mode,
        "merge_layout_blocks": settings.paddle_full_merge_layout_blocks,
        "ocr_image_blocks": settings.paddle_full_ocr_image_blocks,
        "format_block_content": settings.paddle_full_format_block_content,
        "merge_tables": settings.paddle_full_merge_tables,
        "relevel_titles": settings.paddle_full_relevel_titles,
        "show_formula_numbers": settings.paddle_full_show_formula_numbers,
    }))
}

pub(in crate::pipeline::extract) fn activate_paddle_full_cache(
    root: &Path,
    fingerprint: &str,
) -> Result<(), String> {
    let active = serde_json::json!({
        "schema": EXTRACTION_CACHE_SCHEMA,
        "fingerprint": fingerprint,
    });
    atomic_write_cache(
        &root.join(PADDLE_FULL_ACTIVE_FILE),
        &serde_json::to_vec_pretty(&active)
            .map_err(|error| format!("Failed to serialize parser cache activation: {error}"))?,
    )
}
