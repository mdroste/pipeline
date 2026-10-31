//! PDF extraction entry points and the existing managed full-parser owner.

use super::*;

mod cache;
mod full_structure;
mod llm;
mod native;
mod paddle_server;
mod page_quality;

#[path = "paddle_full.rs"]
mod full;

pub(super) use cache::{activate_paddle_full_cache, full_parser_cache_fingerprint};
pub(super) use full::extract_paddle_full;
pub(super) use full_structure::full_parser_extraction_from_structure;
pub(crate) use full_structure::is_withheld_image_text_note;
pub(super) use llm::extract_llm;
pub(crate) use native::extract_pdftotext;
pub(super) use native::{find_command, pdftotext_page_metrics};
pub(super) use paddle_server::{run_paddle_full_sidecar, start_paddle_server};

#[cfg(test)]
pub(super) use full_structure::render_paddle_full_page;
#[cfg(test)]
pub(super) use llm::{extraction_requirements, provider_uses_direct_api};
#[cfg(test)]
pub(super) use paddle_server::{
    paddle_model_list_has_alias, paddle_server_token, PADDLE_MODEL_ALIAS,
    PADDLE_READINESS_POLL_INTERVAL,
};
#[cfg(test)]
pub(super) use page_quality::{
    baseline_page_lengths, baseline_page_metrics, find_suspect_pages, group_into_ranges,
    llm_initial_ranges, parse_page_sections, rebuild_from_sections, split_ranges,
    strip_markdown_fence, PdfPageTextBaseline,
};
