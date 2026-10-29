//! Configured PDF policy and platform-dependent Paddle tuning resolution.

use super::types::Settings;

/// Resolve the global automatic PDF policy without introducing a fallback
/// between extractors. The chosen method remains authoritative for the run.
pub(crate) fn resolve_pdf_extractor(configured: &str) -> &str {
    if configured == "auto" {
        resolve_pdf_extractor_for_paddle_availability(
            configured,
            crate::engines::paddle_full_parser_status().is_ok(),
        )
    } else {
        configured
    }
}

pub(super) fn resolve_pdf_extractor_for_paddle_availability(
    configured: &str,
    paddle_installed: bool,
) -> &str {
    if configured == "auto" {
        if paddle_installed {
            "paddleocr-vl-full"
        } else {
            "llm"
        }
    } else {
        configured
    }
}

/// Resolve Paddle's automatic settings without persisting machine-specific
/// values. Apple Silicon generally has enough unified-memory bandwidth to
/// benefit from two slots and a larger vision-prefill batch. Conservative
/// defaults remain preferable on other platforms, where GPU availability is
/// not reliably discoverable before llama.cpp starts.
pub fn resolved_paddle_page_concurrency(settings: &Settings) -> u32 {
    match settings.paddle_page_concurrency {
        0 if cfg!(all(target_os = "macos", target_arch = "aarch64")) => 2,
        0 => 1,
        value => value,
    }
}

pub fn resolved_paddle_mtmd_batch_tokens(settings: &Settings) -> u32 {
    match settings.paddle_mtmd_batch_tokens {
        0 if cfg!(all(target_os = "macos", target_arch = "aarch64")) => 2048,
        0 => 1024,
        value => value,
    }
}
