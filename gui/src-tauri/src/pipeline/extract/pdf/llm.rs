//! Chunked LLM PDF transcription, verification, and targeted repair.

use super::super::core::{extraction_log, read_utf8_capped, stage_pdf_input};
use super::super::document::scan_math_quality;
use super::super::structured::atomic_write_cache;
use super::cache::llm_extraction_cache_path;
use super::native::pdftotext_page_baseline;
use super::page_quality::{
    find_suspect_pages, group_into_ranges, llm_initial_ranges, parse_page_sections,
    rebuild_from_sections, split_ranges,
};
use crate::models::ExtractionResult;
use std::path::{Path, PathBuf};

/// Max output tokens for extraction calls on direct-API paths. Analysis
/// steps keep the smaller default; transcribing a whole paper needs more.
pub(in crate::pipeline::extract) const EXTRACTION_MAX_OUTPUT_TOKENS: u32 = 32_768;

/// Bound individual LLM transcription calls so the response cap cannot
/// silently cut off a long economics paper. Two ranges run concurrently;
/// the document-level timeout remains the hard cost/latency ceiling.
pub(in crate::pipeline::extract) const LLM_CHUNK_MAX_PAGES: usize = 8;
pub(in crate::pipeline::extract) const LLM_CHUNK_TARGET_BASELINE_CHARS: usize = 50_000;
pub(in crate::pipeline::extract) const LLM_EXTRACTION_CONCURRENCY: usize = 2;

/// Cloud API connections attach the PDF to the request. Subscription and
/// local connections read it through the Read tool.
pub(in crate::pipeline::extract) fn provider_uses_direct_api(
    settings: &crate::settings::Settings,
) -> bool {
    crate::connections::ConnectionId::for_review(&settings.preferred_provider, settings)
        .ok()
        .and_then(|connection| connection.api())
        .is_some_and(|provider| provider != crate::connections::ApiProvider::Local)
}

/// The shared requirements block for extraction and repair prompts.
pub(in crate::pipeline::extract) fn extraction_requirements() -> &'static str {
    "- Start each page's content with the marker <!-- PAGE n --> (1-based page number)\n\
     - Reproduce ALL text content, including abstract, all sections, footnotes, references, and appendices\n\
     - Preserve spelling, wording, and typographical errors verbatim; do not silently correct the paper\n\
     - Preserve mathematical notation using LaTeX syntax (inline $...$ and display $$...$$)\n\
     - Format tables using markdown table syntax\n\
     - Note figure/table captions and their numbers\n\
     - Preserve section numbering and hierarchy\n\
     - Drop running headers, footers, and page numbers\n\
     - Do not summarize or skip any content — output the complete text\n\
     - Do not add commentary, analysis, or annotations — just the extracted text"
}

/// How the prompt tells the model where the document is, depending on
/// whether it will be attached to the request or read via the Read tool.
pub(in crate::pipeline::extract) fn source_line(attach: bool, prompt_path: &str) -> String {
    if attach {
        "The document is attached to this message as a PDF.".to_string()
    } else {
        format!("Read the PDF file at {prompt_path}.")
    }
}

/// Request one bounded, explicit page range. Every requested page marker is
/// mandatory, including blank/figure-only pages, because page completeness is
/// a contract rather than a best-effort quality note.
#[allow(clippy::too_many_arguments)]
pub(in crate::pipeline::extract) async fn request_llm_pages(
    app: crate::emit::EventBus,
    path: PathBuf,
    prompt_path: String,
    start: u32,
    end: u32,
    attach: bool,
    timeout_secs: u64,
    read_dirs: Vec<String>,
    settings: std::sync::Arc<crate::settings::Settings>,
) -> Result<std::collections::BTreeMap<u32, String>, String> {
    let span = if start == end {
        format!("page {start}")
    } else {
        format!("pages {start} through {end}")
    };
    let prompt = format!(
        "{} Transcribe ONLY {span} to well-formatted Markdown.\n\nRequirements:\n{}\n\n{}",
        source_line(attach, &prompt_path),
        extraction_requirements(),
        crate::output::text_artifact_output_format(None, "Markdown transcription")
    );
    let label = format!("LLM PDF extraction (pages {start}-{end})");
    let mut request = crate::pipeline::call::OwnedRequest::new(
        &app,
        format!("extraction-pages-{start}-{end}"),
        label,
        prompt,
        timeout_secs,
    );
    if !attach {
        request.tools = vec!["Read".to_string()];
        request.read_dirs = read_dirs;
    }
    request.pdf_attachment = attach.then_some(path);
    request.max_output_tokens = Some(EXTRACTION_MAX_OUTPUT_TOKENS);
    request.output_schema = Some(crate::output::text_artifact_schema());
    request.settings = settings;
    let raw = crate::pipeline::call::execute_text(request).await?;
    let text = crate::output::extract_text_artifact(&raw)?;
    let (_, sections) = parse_page_sections(&text)
        .ok_or_else(|| format!("LLM response for pages {start}-{end} contained no page markers"))?;
    let missing: Vec<u32> = (start..=end)
        .filter(|page| !sections.contains_key(page))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "LLM response for pages {start}-{end} omitted page marker(s): {}",
            missing
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(sections
        .into_iter()
        .filter(|(page, _)| *page >= start && *page <= end)
        .collect())
}

/// Run the chunked extraction calls. A failed range does not abort the pass:
/// its error is collected and its pages stay absent from the returned map,
/// which makes them suspects for the caller's targeted retry. Only
/// cancellation aborts outright.
#[allow(clippy::too_many_arguments)]
pub(in crate::pipeline::extract) async fn run_llm_ranges(
    app: &crate::emit::EventBus,
    path: &Path,
    prompt_path: &str,
    ranges: &[(u32, u32)],
    attach: bool,
    timeout_secs: u64,
    read_dirs: &[String],
    settings: std::sync::Arc<crate::settings::Settings>,
) -> Result<(std::collections::BTreeMap<u32, String>, Vec<String>), String> {
    let mut tasks = tokio::task::JoinSet::new();
    let mut next = 0usize;
    let mut sections = std::collections::BTreeMap::new();
    let mut range_errors = Vec::new();
    while next < ranges.len() && tasks.len() < LLM_EXTRACTION_CONCURRENCY {
        let (start, end) = ranges[next];
        crate::run_scope::spawn_in(
            &mut tasks,
            request_llm_pages(
                app.clone(),
                path.to_path_buf(),
                prompt_path.to_string(),
                start,
                end,
                attach,
                timeout_secs,
                read_dirs.to_vec(),
                settings.clone(),
            ),
        );
        next += 1;
    }
    while let Some(result) = tasks.join_next().await {
        if crate::commands::is_cancelled() {
            tasks.abort_all();
            return Err("Pipeline cancelled".to_string());
        }
        match result {
            Ok(Ok(range_sections)) => sections.extend(range_sections),
            Ok(Err(error)) => {
                if crate::commands::is_pipeline_cancellation_error(&error) {
                    tasks.abort_all();
                    return Err(error);
                }
                if crate::pipeline::provider_error::is_usage_limit_error(&error)
                    || crate::pipeline::provider_error::is_non_retryable_error(&error)
                {
                    tasks.abort_all();
                    return Err(error);
                }
                extraction_log(app, format!("WARNING: extraction range failed: {error}"));
                range_errors.push(error);
            }
            Err(join_error) => {
                let error = format!("LLM extraction task failed: {join_error}");
                extraction_log(app, format!("WARNING: {error}"));
                range_errors.push(error);
            }
        }
        if next < ranges.len() {
            let (start, end) = ranges[next];
            crate::run_scope::spawn_in(
                &mut tasks,
                request_llm_pages(
                    app.clone(),
                    path.to_path_buf(),
                    prompt_path.to_string(),
                    start,
                    end,
                    attach,
                    timeout_secs,
                    read_dirs.to_vec(),
                    settings.clone(),
                ),
            );
            next += 1;
        }
    }
    Ok((sections, range_errors))
}

/// Extract from a PDF using an LLM (Claude, Codex, or Antigravity).
///
/// Direct-API providers get the PDF attached to the request; CLI providers
/// read it with their multimodal Read tool. The output is verified against
/// a local per-page text-layer map. Calls are proactively chunked, suspicious
/// pages are retried in smaller ranges, and any remaining gap fails before
/// orientation.
pub(in crate::pipeline::extract) async fn extract_llm(
    app: &crate::emit::EventBus,
    path: &Path,
    hash: &str,
) -> Result<ExtractionResult, String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let original_source_path = path_str.to_string();

    let settings = crate::settings::load();
    let attach = provider_uses_direct_api(&settings);
    if settings.preferred_provider == "local" {
        return Err(
            "LLM PDF extraction is not supported by local OpenAI-compatible servers. \
             Use PaddleOCR-VL or pdftotext extraction instead."
                .to_string(),
        );
    }
    let timeout = (settings.step_timeout_secs / 2).clamp(60, 600);
    let settings = std::sync::Arc::new(settings);

    // Stage the PDF first: every helper child (pdftotext for the baseline,
    // provider CLIs) reads the private copy, never the original path. The
    // in-process copy is covered by the user's file-picker grant, while a
    // child reading the original can be silently denied on macOS (TCC).
    let provider_input = tempfile::Builder::new()
        .prefix("pipeline_pdf_provider_input_")
        .tempdir()
        .map_err(|error| format!("Failed to create private PDF input directory: {error}"))?;
    let source = path.to_path_buf();
    let input_root = provider_input.path().to_path_buf();
    let staged_path =
        crate::run_scope::spawn_blocking(move || stage_pdf_input(&source, &input_root))
            .await
            .map_err(|error| format!("PDF staging task failed: {error}"))??;

    // Completeness baseline (poppler is bundled so this is normally
    // available; the error carries pdftotext's own diagnosis, e.g. an
    // encrypted PDF's "Incorrect password").
    let baseline = {
        let p = staged_path.clone();
        crate::run_scope::spawn_blocking(move || pdftotext_page_baseline(&p))
            .await
            .map_err(|error| format!("pdftotext baseline task failed: {error}"))?
    }
    .map_err(|error| {
        format!(
            "LLM extraction could not build the local page-completeness map ({error}); \
             refusing to run an unverified whole-document transcription"
        )
    })?;
    let cache_path = settings
        .reuse_pdf_extraction_cache
        .then(|| llm_extraction_cache_path(hash, &settings))
        .flatten();
    if let Some(path) = &cache_path {
        if let Ok(cached) = read_utf8_capped(path, crate::pipeline::claude::MAX_STDOUT_BYTES) {
            if let Some((_, sections)) = parse_page_sections(&cached) {
                if find_suspect_pages(&sections, &baseline).is_empty() {
                    extraction_log(
                        app,
                        format!(
                            "LLM extraction: reused a verified cached transcription for {hash}"
                        ),
                    );
                    let mut quality_notes = scan_math_quality(&cached);
                    quality_notes.sort();
                    quality_notes.dedup();
                    return Ok(ExtractionResult {
                        text: cached,
                        method: "llm".to_string(),
                        source_path: original_source_path,
                        paper_hash: hash.to_string(),
                        quality_notes,
                    });
                }
            }
        }
    }

    // Anthropic and OpenAI cap PDF inputs at 100 pages per request, and the
    // attach path sends the whole file with every chunk call; base64 also
    // inflates the body by 4/3, so bound the raw size well under their 32 MB
    // request limit. Fail before the first model call with the actual
    // constraint instead of a raw provider HTTP error mid-run.
    if attach && matches!(settings.preferred_provider.as_str(), "claude" | "codex") {
        const MAX_ATTACH_PAGES: usize = 100;
        const MAX_ATTACH_RAW_BYTES: u64 = 24 * 1024 * 1024;
        let bytes = std::fs::metadata(&staged_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        if baseline.len() > MAX_ATTACH_PAGES || bytes > MAX_ATTACH_RAW_BYTES {
            return Err(format!(
                "This PDF ({} pages, {} MB) is over the direct-API attachment limits for LLM \
                 extraction ({MAX_ATTACH_PAGES} pages, {} MB). Use a CLI provider, or \
                 PaddleOCR-VL / pdftotext extraction instead.",
                baseline.len(),
                bytes / 1024 / 1024,
                MAX_ATTACH_RAW_BYTES / 1024 / 1024
            ));
        }
    }

    let prompt_path = staged_path.to_string_lossy().replace('\\', "/");
    // Direct APIs receive the selected PDF as an attachment and need no file
    // tool. CLI transports receive exactly one private read root.
    let read_dirs = (!attach)
        .then(|| provider_input.path().to_string_lossy().replace('\\', "/"))
        .into_iter()
        .collect::<Vec<_>>();

    let ranges = llm_initial_ranges(&baseline);
    extraction_log(
        app,
        format!(
            "LLM extraction: {} pages in {} bounded range(s), up to {} concurrent",
            baseline.len(),
            ranges.len(),
            LLM_EXTRACTION_CONCURRENCY
        ),
    );
    let (mut sections, mut range_errors) = run_llm_ranges(
        app,
        &staged_path,
        &prompt_path,
        &ranges,
        attach,
        timeout,
        &read_dirs,
        settings.clone(),
    )
    .await?;

    // Failed ranges left their pages out of `sections`, so they surface here
    // as suspects alongside implausibly short pages, and the targeted retry
    // pass re-requests both in smaller ranges.
    let suspects = find_suspect_pages(&sections, &baseline);
    if !suspects.is_empty() {
        let retry_ranges = split_ranges(&group_into_ranges(&suspects), 2);
        extraction_log(
            app,
            format!(
                "Extraction verification: retrying {} suspect page(s) in {} smaller range(s)",
                suspects.len(),
                retry_ranges.len()
            ),
        );
        let (retried, retry_errors) = run_llm_ranges(
            app,
            &staged_path,
            &prompt_path,
            &retry_ranges,
            attach,
            timeout,
            &read_dirs,
            settings,
        )
        .await?;
        sections.extend(retried);
        range_errors.extend(retry_errors);
    }

    let still_suspect = find_suspect_pages(&sections, &baseline);
    if !still_suspect.is_empty() {
        let detail = range_errors
            .last()
            .map(|error| format!(" (last range error: {error})"))
            .unwrap_or_default();
        return Err(format!(
            "LLM extraction remained incomplete after targeted retry on page(s): {}{detail}",
            still_suspect
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let final_text = rebuild_from_sections("", &sections);
    let mut quality_notes = Vec::new();
    quality_notes.extend(scan_math_quality(&final_text));
    if let Some(path) = cache_path {
        if let Err(error) = atomic_write_cache(&path, final_text.as_bytes()) {
            extraction_log(
                app,
                format!("WARNING: could not save verified LLM extraction cache: {error}"),
            );
        }
    }

    Ok(ExtractionResult {
        text: final_text,
        method: "llm".to_string(),
        source_path: original_source_path,
        paper_hash: hash.to_string(),
        quality_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_scope::fixtures::{hanging_provider, until_called};

    #[tokio::test]
    async fn stopping_a_review_stops_its_extraction_ranges_and_keeps_their_usage() {
        let (url, calls) = hanging_provider();
        let settings = crate::settings::Settings {
            preferred_provider: "local".into(),
            local_base_url: url,
            local_model: "fixture".into(),
            ..Default::default()
        };
        let bus: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        let run = crate::run_scope::ActiveScope::begin("extraction-range-stop-test");

        let ranges = crate::run_scope::scoped(
            run.scope().clone(),
            run_llm_ranges(
                &bus,
                Path::new("paper.pdf"),
                "paper.pdf",
                &[(1, 1), (2, 2)],
                false,
                300,
                &[],
                std::sync::Arc::new(settings),
            ),
        );
        let stop = async {
            until_called(&calls, 2).await;
            run.scope().cancel();
        };
        let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(40), async {
            tokio::join!(ranges, stop)
        })
        .await
        .expect("stopped extraction must return");

        assert!(crate::commands::is_pipeline_cancellation_error(
            &result.unwrap_err()
        ));
        assert_eq!(run.scope().usage().provider_attempts, 2);
    }
}
