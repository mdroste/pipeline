use super::*;

/// Extract text from PDF using pdftotext. The child reads a staged private
/// copy, never the original path (macOS TCC: helper children reading the
/// user's Documents/Desktop need their own folder permission; the in-process
/// staging copy is covered by the file-picker grant).
pub(crate) fn extract_pdftotext(path: &Path) -> Result<String, String> {
    let staging = tempfile::Builder::new()
        .prefix("pipeline_pdf_text_input_")
        .tempdir()
        .map_err(|error| format!("Failed to create private PDF input directory: {error}"))?;
    let staged = stage_pdf_input(path, staging.path())?;
    let path_str = staged
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", staged.display()))?;
    let pdftotext_bin = find_command("pdftotext").ok_or("pdftotext not found on PATH")?;
    let mut command = pdftotext_bin.command(["-layout", path_str, "-"]);
    command.env("PATH", env::full_path());
    let output = run_bounded_output(
        command,
        "pdftotext",
        std::time::Duration::from_secs(crate::settings::load().step_timeout_secs.max(60)),
        crate::pipeline::claude::MAX_STDOUT_BYTES,
        None,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let suffix = if output.stderr_truncated {
            " (truncated)"
        } else {
            ""
        };
        return Err(format!("pdftotext failed: {}{suffix}", stderr.trim()));
    }
    if output.stdout_truncated {
        return Err("pdftotext output exceeded the 50 MB safety limit".to_string());
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("pdftotext returned empty output".to_string());
    }
    Ok(text)
}

pub(super) const PADDLE_MODEL_ALIAS: &str = "paddleocr-vl-1.6";
pub(super) const MAX_PADDLE_MODEL_LIST_BYTES: usize = 1024 * 1024;
pub(super) const PADDLE_READINESS_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(250);

pub(super) fn paddle_server_token() -> Result<String, String> {
    use std::fmt::Write as _;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("Failed to generate managed server credentials: {error}"))?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(token)
}

pub(super) fn paddle_model_list_has_alias(value: &serde_json::Value) -> bool {
    value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|models| {
            models.iter().any(|model| {
                model.get("id").and_then(serde_json::Value::as_str) == Some(PADDLE_MODEL_ALIAS)
            })
        })
}

pub(super) struct PaddleServer {
    child: std::process::Child,
    pid: u32,
    base_url: String,
    api_key: String,
}

impl Drop for PaddleServer {
    fn drop(&mut self) {
        if self.pid > 0 {
            crate::commands::kill_process(self.pid);
            crate::commands::unregister_child_pid(self.pid);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) async fn start_paddle_server(
    paths: &crate::engines::PaddleEnginePaths,
    settings: &crate::settings::Settings,
) -> Result<PaddleServer, String> {
    use std::process::Stdio;

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Failed to reserve a PaddleOCR-VL port: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Failed to inspect the PaddleOCR-VL port: {error}"))?
        .port();
    drop(listener);

    let model = paths
        .model
        .to_str()
        .ok_or("PaddleOCR-VL model path is not valid UTF-8")?;
    let projector = paths
        .mmproj
        .to_str()
        .ok_or("PaddleOCR-VL projector path is not valid UTF-8")?;
    let port = port.to_string();
    let resolved_concurrency = crate::settings::resolved_paddle_page_concurrency(settings);
    let page_concurrency = resolved_concurrency.to_string();
    // llama.cpp's --ctx-size is the total KV budget shared by all parallel
    // slots. Scale it with concurrency so a two-page run still gives every
    // page the model's full 16K context.
    let total_context = (16_384u32 * resolved_concurrency).to_string();
    let mtmd_batch_tokens =
        crate::settings::resolved_paddle_mtmd_batch_tokens(settings).to_string();
    let api_key = paddle_server_token()?;
    let mut command = StdCommand::new(&paths.server);
    command.args([
        "-m",
        model,
        "--mmproj",
        projector,
        "--host",
        "127.0.0.1",
        "--port",
        &port,
        "--temp",
        "0",
        "--ctx-size",
        &total_context,
        "--n-gpu-layers",
        "99",
        "--parallel",
        &page_concurrency,
        "--mtmd-batch-max-tokens",
        &mtmd_batch_tokens,
        "--flash-attn",
        &settings.paddle_flash_attention,
        "--alias",
        PADDLE_MODEL_ALIAS,
        "--api-key",
        &api_key,
        "--no-ui",
    ]);
    crate::pipeline::claude::configure_silent_command(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start managed llama-server: {error}"))?;
    let pid = child.id();
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }
    let base_url = format!("http://127.0.0.1:{port}");
    let client = &*crate::pipeline::api_common::LOCAL_HTTP_CLIENT;
    let started = std::time::Instant::now();
    loop {
        if crate::commands::is_cancelled() {
            if pid > 0 {
                crate::commands::kill_process(pid);
                crate::commands::unregister_child_pid(pid);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("Pipeline cancelled".to_string());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to inspect llama-server: {error}"))?
        {
            if pid > 0 {
                crate::commands::unregister_child_pid(pid);
            }
            return Err(format!(
                "Managed llama-server exited during model loading (exit {})",
                status.code().unwrap_or(-1)
            ));
        }
        if let Ok(response) = client
            .get(format!("{base_url}/health"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            if response.status().is_success() {
                let identity = client
                    .get(format!("{base_url}/v1/models"))
                    .bearer_auth(&api_key)
                    .timeout(std::time::Duration::from_secs(2))
                    .send()
                    .await;
                if let Ok(identity) = identity {
                    if identity.status().is_success() {
                        if let Ok(bytes) = crate::pipeline::api_common::response_bytes_limited(
                            identity,
                            MAX_PADDLE_MODEL_LIST_BYTES,
                            None,
                        )
                        .await
                        {
                            if serde_json::from_slice::<serde_json::Value>(&bytes)
                                .ok()
                                .as_ref()
                                .is_some_and(paddle_model_list_has_alias)
                            {
                                return Ok(PaddleServer {
                                    child,
                                    pid,
                                    base_url,
                                    api_key,
                                });
                            }
                        }
                    }
                }
            }
        }
        if started.elapsed() >= std::time::Duration::from_secs(180) {
            if pid > 0 {
                crate::commands::kill_process(pid);
                crate::commands::unregister_child_pid(pid);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("PaddleOCR-VL model loading timed out after 180 seconds".to_string());
        }
        let _ = crate::commands::await_or_cancel(
            tokio::time::sleep(PADDLE_READINESS_POLL_INTERVAL),
            None,
        )
        .await;
    }
}

pub(super) fn paddle_substantive_chars(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.contains(PADDLE_REGION_WARNING_PREFIX))
        .flat_map(str::chars)
        .filter(|value| !value.is_whitespace())
        .count()
}

pub(super) fn paddle_char_count_is_suspicious(
    extracted_chars: usize,
    baseline_len: Option<usize>,
) -> bool {
    baseline_len.is_some_and(|baseline_len| {
        baseline_len >= SUSPECT_BASELINE_MIN_CHARS && extracted_chars < baseline_len / 10
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PdfPageTextBaseline {
    pub(super) total_chars: usize,
    pub(super) prose_chars: usize,
}

fn paddle_page_is_figure_related(page: &PaddleStructuredPage) -> bool {
    let mut has_figure_region = false;
    for block in &page.blocks {
        match block.role.to_ascii_lowercase().as_str() {
            "table" | "table_title" => return false,
            "chart" | "image" | "figure" | "figure_title" | "chart_body" | "image_body"
            | "picture" => has_figure_region = true,
            _ => {}
        }
    }
    has_figure_region
}

fn paddle_effective_baseline(
    page: &PaddleStructuredPage,
    baseline: PdfPageTextBaseline,
) -> (usize, bool) {
    let fragmented_visual_page = baseline.total_chars >= SUSPECT_BASELINE_MIN_CHARS
        && baseline.prose_chars < baseline.total_chars / 2
        && paddle_page_is_figure_related(page);
    if fragmented_visual_page {
        (baseline.prose_chars, true)
    } else {
        (baseline.total_chars, false)
    }
}

fn fragmented_visual_baseline_note(page: u32, baseline: PdfPageTextBaseline) -> String {
    format!(
        "Page {page}: the PDF text layer is dominated by isolated figure labels; completeness was checked against {} prose-like characters rather than {} total characters.",
        baseline.prose_chars, baseline.total_chars
    )
}

pub(super) fn llm_extraction_cache_path(
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
        "model_policy": "automatic",
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

pub(super) fn full_parser_cache_fingerprint(
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
        "recognition_integrity": paths.paddle.integrity_sha256,
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

pub(super) fn paddle_full_block_markdown(page: &PaddleStructuredPage) -> String {
    page.blocks
        .iter()
        .filter(|block| {
            !matches!(
                block.role.as_str(),
                "number" | "header" | "header_image" | "footer" | "footer_image"
            )
        })
        .map(|block| block.markdown.trim())
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(super) fn paddle_full_best_markdown(page: &PaddleStructuredPage) -> String {
    let exported = page.markdown.trim();
    let blocks = paddle_full_block_markdown(page);
    if exported.is_empty()
        || (!blocks.is_empty()
            && paddle_substantive_chars(exported).saturating_mul(2)
                < paddle_substantive_chars(&blocks))
    {
        blocks
    } else {
        exported.to_string()
    }
}

pub(super) fn render_paddle_full_page(page: &PaddleStructuredPage) -> String {
    let markdown = paddle_full_best_markdown(page);
    let markdown = markdown
        .replace("](assets/", "](../artifacts/figures/")
        .replace("src=\"assets/", "src=\"../artifacts/figures/")
        .replace("src='assets/", "src='../artifacts/figures/");
    format!("<!-- PAGE {} -->\n\n{}", page.number, markdown)
        .trim()
        .to_string()
}

pub(super) fn paddle_full_page_has_content(page: &PaddleStructuredPage) -> bool {
    if !page.markdown.trim().is_empty() {
        return true;
    }
    page.blocks.iter().any(|block| {
        !matches!(
            block.role.as_str(),
            "number" | "header" | "header_image" | "footer" | "footer_image"
        ) && (!block.markdown.trim().is_empty()
            || !block.text.trim().is_empty()
            || !block.asset_files.is_empty())
    })
}

pub(super) fn full_parser_extraction_from_structure(
    structure: &PaddleStructure,
    path: &Path,
    hash: &str,
    baseline: Option<&[PdfPageTextBaseline]>,
) -> Result<ExtractionResult, String> {
    if baseline.is_some_and(|pages| pages.len() > MAX_RENDERED_PDF_PAGES as usize) {
        return Err(format!(
            "PaddleOCR-VL Full Parser is limited to {MAX_RENDERED_PDF_PAGES} pages per PDF"
        ));
    }
    if structure.schema_version != PADDLE_STRUCTURE_SCHEMA
        || structure.parser != "paddleocr-vl-full"
        || structure.pages.is_empty()
    {
        return Err("PaddleOCR-VL full parser returned an incompatible structure".to_string());
    }
    if let Some(baseline) = baseline {
        if structure.pages.len() != baseline.len() {
            return Err(format!(
                "PaddleOCR-VL full parser returned {} pages for a {}-page PDF",
                structure.pages.len(),
                baseline.len()
            ));
        }
    }
    let mut restructured_empty_pages = Vec::new();
    let mut fragmented_visual_notes = Vec::new();
    for (index, page) in structure.pages.iter().enumerate() {
        let expected = index as u32 + 1;
        if page.number != expected {
            return Err(format!(
                "PaddleOCR-VL full parser returned page {} where page {expected} was expected",
                page.number
            ));
        }
        if !paddle_full_page_has_content(page) && page.source_text_chars.unwrap_or(0) == 0 {
            return Err(format!(
                "PaddleOCR-VL full parser returned no content for page {expected}"
            ));
        }
        if !paddle_full_page_has_content(page) {
            restructured_empty_pages.push(expected);
        }
        let rendered = render_paddle_full_page(page);
        let recognized_chars = page
            .source_text_chars
            .unwrap_or_else(|| paddle_substantive_chars(&rendered));
        let page_baseline = baseline.and_then(|pages| pages.get(index)).copied();
        let (baseline_chars, used_fragmented_visual_baseline) = page_baseline
            .map(|value| paddle_effective_baseline(page, value))
            .unwrap_or((0, false));
        if paddle_char_count_is_suspicious(recognized_chars, Some(baseline_chars)) {
            if used_fragmented_visual_baseline {
                return Err(format!(
                    "PaddleOCR-VL full parser returned incomplete output for page {expected} (recognized {recognized_chars} substantive characters; prose-like text baseline has {baseline_chars}; full text layer has {})",
                    page_baseline.map_or(0, |value| value.total_chars)
                ));
            }
            return Err(format!(
                "PaddleOCR-VL full parser returned incomplete output for page {expected} (recognized {recognized_chars} substantive characters; text layer has {baseline_chars})"
            ));
        }
        if used_fragmented_visual_baseline
            && paddle_char_count_is_suspicious(
                recognized_chars,
                page_baseline.map(|value| value.total_chars),
            )
        {
            fragmented_visual_notes.push(fragmented_visual_baseline_note(
                expected,
                page_baseline.unwrap_or_default(),
            ));
        }
    }
    let text = structure
        .pages
        .iter()
        .map(render_paddle_full_page)
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.len() > crate::pipeline::claude::MAX_STDOUT_BYTES {
        return Err(
            "PaddleOCR-VL full-parser document exceeded the 50 MB safety limit".to_string(),
        );
    }
    let mut quality_notes = structure.quality_notes.clone();
    for note in fragmented_visual_notes {
        if !quality_notes.contains(&note) {
            quality_notes.push(note);
        }
    }
    if !restructured_empty_pages.is_empty() {
        quality_notes.push(format!(
            "PaddleOCR cross-page restructuring moved all recognized content from page(s) {} into adjacent structured blocks.",
            restructured_empty_pages
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    quality_notes.extend(scan_math_quality(&text));
    Ok(ExtractionResult {
        text,
        method: "paddleocr-vl-full".to_string(),
        source_path: path.to_string_lossy().to_string(),
        paper_hash: hash.to_string(),
        quality_notes,
    })
}

pub(super) fn activate_paddle_full_cache(root: &Path, fingerprint: &str) -> Result<(), String> {
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

#[allow(clippy::too_many_arguments)]
pub(super) fn run_paddle_full_sidecar(
    paths: &crate::engines::PaddleFullParserPaths,
    settings: &crate::settings::Settings,
    input: &Path,
    output: &Path,
    assets: &Path,
    base_url: &str,
    api_key: &str,
    baselines: &[PdfPageTextBaseline],
    timeout: std::time::Duration,
) -> Result<BoundedOutput, String> {
    let boolean = |value: bool| if value { "true" } else { "false" };
    let baseline_chars = baselines
        .iter()
        .map(|baseline| baseline.total_chars)
        .collect::<Vec<_>>();
    let baseline_prose_chars = baselines
        .iter()
        .map(|baseline| baseline.prose_chars)
        .collect::<Vec<_>>();
    let baseline_chars = serde_json::to_string(&baseline_chars)
        .map_err(|error| format!("Failed to serialize PDF text-layer baseline: {error}"))?;
    let baseline_prose_chars = serde_json::to_string(&baseline_prose_chars)
        .map_err(|error| format!("Failed to serialize PDF prose-line baseline: {error}"))?;
    let mut command = StdCommand::new(&paths.python);
    let arguments: Vec<std::ffi::OsString> = vec![
        "-I".into(),
        "-B".into(),
        paths.script.as_os_str().to_owned(),
        "--input".into(),
        input.as_os_str().to_owned(),
        "--output".into(),
        output.as_os_str().to_owned(),
        "--assets-dir".into(),
        assets.as_os_str().to_owned(),
        "--layout-model-dir".into(),
        paths.layout_model.as_os_str().to_owned(),
        "--server-url".into(),
        base_url.into(),
        "--server-model".into(),
        PADDLE_MODEL_ALIAS.into(),
        "--concurrency".into(),
        crate::settings::resolved_paddle_page_concurrency(settings)
            .to_string()
            .into(),
        "--max-new-tokens".into(),
        settings.paddle_max_output_tokens.to_string().into(),
        "--page-retries".into(),
        settings.paddle_page_retries.to_string().into(),
        "--baseline-chars".into(),
        baseline_chars.into(),
        "--baseline-prose-chars".into(),
        baseline_prose_chars.into(),
        "--layout-detection".into(),
        boolean(settings.paddle_full_layout_detection).into(),
        "--layout-threshold".into(),
        settings.paddle_full_layout_threshold.to_string().into(),
        "--layout-nms".into(),
        boolean(settings.paddle_full_layout_nms).into(),
        "--layout-merge-bboxes-mode".into(),
        settings.paddle_full_layout_merge_bboxes_mode.clone().into(),
        "--merge-layout-blocks".into(),
        boolean(settings.paddle_full_merge_layout_blocks).into(),
        "--ocr-image-blocks".into(),
        boolean(settings.paddle_full_ocr_image_blocks).into(),
        "--format-block-content".into(),
        boolean(settings.paddle_full_format_block_content).into(),
        "--merge-tables".into(),
        boolean(settings.paddle_full_merge_tables).into(),
        "--relevel-titles".into(),
        boolean(settings.paddle_full_relevel_titles).into(),
        "--show-formula-numbers".into(),
        boolean(settings.paddle_full_show_formula_numbers).into(),
    ];
    command.args(arguments);
    let mut environment = crate::engines::paddle_full_parser_env(paths);
    environment.push(("PIPELINE_PADDLE_API_KEY".to_string(), api_key.to_string()));
    crate::engines::apply_managed_environment(&mut command, &environment);
    if let Some(parent) = output.parent() {
        command.current_dir(parent);
    }
    run_bounded_output(
        command,
        "PaddleOCR-VL full parser",
        timeout,
        4 * 1024 * 1024,
        output.parent(),
    )
}

pub(super) async fn extract_paddle_full(
    app: &crate::emit::EventBus,
    path: &Path,
    hash: &str,
    settings: &crate::settings::Settings,
) -> Result<ExtractionResult, String> {
    let started = std::time::Instant::now();
    let total_timeout = std::time::Duration::from_secs(settings.pdf_extraction_timeout_secs);
    // The first resolution each session content-hashes the multi-gigabyte
    // managed runtime: announce it, keep it off the async workers, and let
    // cancellation return immediately (the detached hash finishes in the
    // background and populates the session verification cache harmlessly).
    extraction_log(
        app,
        "Verifying the PaddleOCR-VL runtime (the first use each session may take a while)…",
    );
    let paths = crate::commands::await_or_cancel(
        tokio::task::spawn_blocking(crate::engines::paddle_full_parser_paths),
        None,
    )
    .await?
    .map_err(|error| format!("PaddleOCR-VL runtime verification task failed: {error}"))??;
    // Stage the PDF first: the pdftotext baseline and the parser sidecar read
    // the private copy, never the original path — a child reading the
    // original can be silently denied on macOS (TCC).
    let staging_input = tempfile::Builder::new()
        .prefix("pipeline_pdf_parser_input_")
        .tempdir()
        .map_err(|error| format!("Failed to create private PDF input directory: {error}"))?;
    let source = path.to_path_buf();
    let input_root = staging_input.path().to_path_buf();
    let staged_pdf = tokio::task::spawn_blocking(move || stage_pdf_input(&source, &input_root))
        .await
        .map_err(|error| format!("PDF staging task failed: {error}"))??;
    // The baseline both verifies parser completeness and enforces the page
    // cap, so it is required: without it a corrupt or encrypted PDF could
    // enter the sidecar unbounded and unverified.
    let baseline = {
        let pdf = staged_pdf.clone();
        tokio::task::spawn_blocking(move || pdftotext_page_metrics(&pdf))
            .await
            .map_err(|error| format!("pdftotext baseline task failed: {error}"))?
    }
    .map_err(|error| {
        format!(
            "PaddleOCR-VL extraction needs the local pdftotext page map to bound and verify \
             parsing, but it could not be built: {error}"
        )
    })?;
    if baseline.len() > MAX_RENDERED_PDF_PAGES as usize {
        return Err(format!(
            "PaddleOCR-VL Full Parser is limited to {MAX_RENDERED_PDF_PAGES} pages per PDF"
        ));
    }
    let baseline = Some(baseline);
    let root = paddle_full_cache_root(hash)
        .ok_or_else(|| "Could not create PaddleOCR-VL full-parser cache path".to_string())?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Failed to create full-parser cache: {error}"))?;
    let fingerprint = full_parser_cache_fingerprint(&paths, settings);
    let target = root.join(&fingerprint);
    let structure_path = target.join(PADDLE_FULL_STRUCTURE_FILE);
    if settings.reuse_pdf_extraction_cache && structure_path.is_file() {
        let text = read_utf8_capped(&structure_path, crate::pipeline::claude::MAX_STDOUT_BYTES)?;
        if let Ok(structure) = serde_json::from_str::<PaddleStructure>(&text) {
            if let Ok(extraction) =
                full_parser_extraction_from_structure(&structure, path, hash, baseline.as_deref())
            {
                // Cache bookkeeping must never fail a successful extraction.
                if let Err(error) = activate_paddle_full_cache(&root, &fingerprint) {
                    extraction_log(
                        app,
                        format!("WARNING: could not record the active parser cache: {error}"),
                    );
                }
                extraction_log(
                    app,
                    "PaddleOCR-VL Full Parser: reused a schema-validated structured cache",
                );
                return Ok(extraction);
            }
        }
    }

    extraction_log(
        app,
        format!(
            "PaddleOCR-VL Full Parser: layout={}, NMS={}, merge blocks={}, image OCR={}, cross-page tables={}, relevel titles={}",
            settings.paddle_full_layout_detection,
            settings.paddle_full_layout_nms,
            settings.paddle_full_merge_layout_blocks,
            settings.paddle_full_ocr_image_blocks,
            settings.paddle_full_merge_tables,
            settings.paddle_full_relevel_titles,
        ),
    );
    extraction_log(
        app,
        "PaddleOCR-VL Full Parser: loading the managed Q8 recognition server",
    );
    let server = start_paddle_server(&paths.paddle, settings).await?;
    let staging = tempfile::Builder::new()
        .prefix(".full-parser-staging-")
        .tempdir_in(&root)
        .map_err(|error| format!("Failed to stage full-parser output: {error}"))?;
    let staged_structure = staging.path().join(PADDLE_FULL_STRUCTURE_FILE);
    let staged_assets = staging.path().join("assets");
    let parser_paths = paths.clone();
    let parser_settings = settings.clone();
    let input = staged_pdf.clone();
    let output = staged_structure.clone();
    let assets = staged_assets.clone();
    let base_url = format!("{}/v1", server.base_url);
    let api_key = server.api_key.clone();
    let baselines = baseline.clone().unwrap_or_default();
    let timeout = total_timeout
        .checked_sub(started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            format!(
                "PaddleOCR-VL Full Parser exceeded the {}s extraction budget while loading",
                settings.pdf_extraction_timeout_secs
            )
        })?;
    let sidecar = tokio::task::spawn_blocking(move || {
        run_paddle_full_sidecar(
            &parser_paths,
            &parser_settings,
            &input,
            &output,
            &assets,
            &base_url,
            &api_key,
            &baselines,
            timeout,
        )
    })
    .await
    .map_err(|error| format!("PaddleOCR-VL full-parser task failed: {error}"))??;
    if !sidecar.status.success() {
        let stderr = String::from_utf8_lossy(&sidecar.stderr);
        let diagnostics = paddle_diagnostic_lines(&stderr, 50).join("\n");
        let diagnostics = if diagnostics.is_empty() {
            ANSI_ESCAPE_SEQUENCE
                .replace_all(stderr.trim(), "")
                .to_string()
        } else {
            diagnostics
        };
        let suffix = if sidecar.stderr_truncated {
            " (truncated)"
        } else {
            ""
        };
        return Err(format!(
            "PaddleOCR-VL full parser failed: {}{suffix}",
            diagnostics.trim()
        ));
    }
    if sidecar.stdout_truncated || sidecar.stderr_truncated {
        extraction_log(
            app,
            "WARNING: PaddleOCR-VL full-parser diagnostic output was truncated",
        );
    }
    let stderr = String::from_utf8_lossy(&sidecar.stderr);
    for line in paddle_diagnostic_lines(&stderr, 50) {
        extraction_log(app, format!("PaddleOCR parser: {line}"));
    }
    let structure_text =
        read_utf8_capped(&staged_structure, crate::pipeline::claude::MAX_STDOUT_BYTES)?;
    let structure: PaddleStructure = serde_json::from_str(&structure_text)
        .map_err(|error| format!("Failed to parse managed full-parser output: {error}"))?;
    let extraction =
        full_parser_extraction_from_structure(&structure, path, hash, baseline.as_deref())?;

    let backup = root.join(format!(".{fingerprint}-backup"));
    if backup.exists() {
        fs::remove_dir_all(&backup)
            .map_err(|error| format!("Failed to clean old parser cache backup: {error}"))?;
    }
    if target.exists() {
        fs::rename(&target, &backup)
            .map_err(|error| format!("Failed to stage prior parser cache: {error}"))?;
    }
    let staged_path = staging.keep();
    if let Err(error) = fs::rename(&staged_path, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(format!("Failed to activate full-parser cache: {error}"));
    }
    // Cache bookkeeping must never fail a successful extraction.
    if let Err(error) = activate_paddle_full_cache(&root, &fingerprint) {
        extraction_log(
            app,
            format!("WARNING: could not record the active parser cache: {error}"),
        );
    }
    if backup.exists() {
        let _ = fs::remove_dir_all(&backup);
    }
    extraction_log(
        app,
        format!(
            "PaddleOCR-VL Full Parser: completed {} structured page(s) in {:.1}s",
            structure.pages.len(),
            started.elapsed().as_secs_f64()
        ),
    );
    Ok(extraction)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn find_command(name: &str) -> Option<crate::deps::ResolvedCommand> {
    crate::deps::resolve_command(name)
}

// ── LLM extraction: transport, verification, repair ─────────────────

/// Max output tokens for extraction calls on direct-API paths. Analysis
/// steps keep the smaller default; transcribing a whole paper needs more.
pub(super) const EXTRACTION_MAX_OUTPUT_TOKENS: u32 = 32_768;

/// Bound individual LLM transcription calls so the response cap cannot
/// silently cut off a long economics paper. Two ranges run concurrently;
/// the document-level timeout remains the hard cost/latency ceiling.
pub(super) const LLM_CHUNK_MAX_PAGES: usize = 8;
pub(super) const LLM_CHUNK_TARGET_BASELINE_CHARS: usize = 50_000;
pub(super) const LLM_EXTRACTION_CONCURRENCY: usize = 2;

/// A page's extraction is suspect when the pdftotext baseline has at least
/// this many characters but the LLM produced less than a quarter of it.
/// The floor keeps figure-heavy pages (thin text layer) from tripping it.
pub(super) const SUSPECT_BASELINE_MIN_CHARS: usize = 200;

/// True when call_llm() will take a direct-API path for the preferred
/// provider — mirrors the dispatch in pipeline::claude::call_llm. Direct
/// API calls get the PDF attached to the request; CLI calls read it via
/// the Read tool.
pub(super) fn provider_uses_direct_api(settings: &crate::settings::Settings) -> bool {
    match settings.preferred_provider.as_str() {
        "claude" | "codex" | "antigravity" => {
            settings.model_transport(&settings.preferred_provider) == "api"
        }
        // Local OpenAI-compatible servers intentionally reject the file-part
        // attachment used by the verified LLM extraction path.
        "local" => false,
        _ => false,
    }
}

/// Per-page character counts from pdftotext output (pages are separated by
/// form feeds). An error names the actual cause — missing binary, non-zero
/// exit with its stderr (an encrypted PDF says "Incorrect password" here),
/// or truncated output — so callers can surface it instead of a generic
/// "could not build the page map".
pub(super) fn pdftotext_page_metrics(path: &Path) -> Result<Vec<PdfPageTextBaseline>, String> {
    let bin = find_command("pdftotext")
        .ok_or_else(|| "pdftotext is not available on PATH".to_string())?;
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Path contains invalid UTF-8: {}", path.display()))?;
    let mut command = bin.command(["-layout", path_str, "-"]);
    command.env("PATH", env::full_path());
    let output = run_bounded_output(
        command,
        "pdftotext baseline",
        std::time::Duration::from_secs(120),
        crate::pipeline::claude::MAX_STDOUT_BYTES,
        None,
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "pdftotext failed ({}): {}",
            output.status,
            stderr.trim()
        ));
    }
    if output.stdout_truncated {
        return Err("pdftotext baseline output exceeded the 50 MB safety limit".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let pages = baseline_page_metrics(&text);
    if pages.is_empty() {
        Err("pdftotext reported no pages".to_string())
    } else {
        Ok(pages)
    }
}

pub(super) fn pdftotext_page_baseline(path: &Path) -> Result<Vec<usize>, String> {
    pdftotext_page_metrics(path)
        .map(|pages| pages.into_iter().map(|page| page.total_chars).collect())
}

/// Split pdftotext output on form feeds and count substantive characters.
/// `-layout` can emit thousands of alignment spaces on a sparse figure page;
/// counting those as source text makes complete OCR look implausibly short.
#[cfg(test)]
pub(super) fn baseline_page_lengths(text: &str) -> Vec<usize> {
    baseline_page_metrics(text)
        .into_iter()
        .map(|page| page.total_chars)
        .collect()
}

/// Measure both the full text layer and ordinary prose-like lines. Plot
/// labels are commonly emitted as thousands of isolated words; they are a
/// useful visual annotation but not a sound completeness target for a
/// layout-aware OCR parser that preserves the rendered page as visual evidence.
pub(super) fn baseline_page_metrics(text: &str) -> Vec<PdfPageTextBaseline> {
    let mut pages: Vec<PdfPageTextBaseline> = text
        .split('\u{0C}')
        .map(|page| {
            let total_chars = page.chars().filter(|value| !value.is_whitespace()).count();
            let prose_chars = page
                .lines()
                .map(|line| {
                    let chars = line.chars().filter(|value| !value.is_whitespace()).count();
                    let words = line.split_whitespace().count();
                    let alphabetic_words = line
                        .split_whitespace()
                        .filter(|word| {
                            word.chars().filter(|value| value.is_alphabetic()).count() >= 2
                        })
                        .count();
                    if words >= 3 && alphabetic_words >= 2 && chars >= 12 {
                        chars
                    } else {
                        0
                    }
                })
                .sum();
            PdfPageTextBaseline {
                total_chars,
                prose_chars,
            }
        })
        .collect();
    // pdftotext terminates every page with a form feed, leaving a trailing
    // empty segment.
    if pages.last().is_some_and(|page| page.total_chars == 0) {
        pages.pop();
    }
    pages
}

pub(super) fn page_marker_regex() -> Regex {
    Regex::new(r"(?i)<!--\s*page\s+(\d+)\s*-->").expect("page marker regex is invalid")
}

/// Split extracted text on `<!-- PAGE n -->` markers into (preamble,
/// page → content). Returns None when the text has no markers at all, in
/// which case completeness cannot be verified. Content under a repeated
/// marker is appended, not replaced.
pub(super) fn parse_page_sections(
    text: &str,
) -> Option<(String, std::collections::BTreeMap<u32, String>)> {
    let re = page_marker_regex();
    let mut sections: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
    let mut preamble = String::new();
    let mut current: Option<u32> = None;
    let mut seg_start = 0usize;
    let mut found = false;

    let push_segment = |current: Option<u32>,
                        segment: &str,
                        sections: &mut std::collections::BTreeMap<u32, String>,
                        preamble: &mut String| {
        match current {
            Some(page) => sections.entry(page).or_default().push_str(segment),
            None => preamble.push_str(segment),
        }
    };

    for cap in re.captures_iter(text) {
        let m = cap.get(0).expect("regex match has group 0");
        push_segment(
            current,
            &text[seg_start..m.start()],
            &mut sections,
            &mut preamble,
        );
        current = cap[1].parse::<u32>().ok();
        seg_start = m.end();
        found = true;
    }
    push_segment(current, &text[seg_start..], &mut sections, &mut preamble);

    if !found {
        return None;
    }
    Some((preamble.trim().to_string(), sections))
}

/// Pages that are missing entirely or came back far shorter than the
/// pdftotext baseline says they should be.
pub(super) fn find_suspect_pages(
    sections: &std::collections::BTreeMap<u32, String>,
    baseline: &[usize],
) -> Vec<u32> {
    let mut suspects = Vec::new();
    for (i, &base_len) in baseline.iter().enumerate() {
        let page = (i + 1) as u32;
        match sections.get(&page) {
            None => suspects.push(page),
            Some(content) => {
                if base_len >= SUSPECT_BASELINE_MIN_CHARS && content.trim().len() < base_len / 4 {
                    suspects.push(page);
                }
            }
        }
    }
    suspects
}

/// Group sorted page numbers into contiguous (start, end) ranges.
pub(super) fn group_into_ranges(pages: &[u32]) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for &p in pages {
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == p => *end = p,
            _ => ranges.push((p, p)),
        }
    }
    ranges
}

/// Reassemble the extraction from its page sections, in page order.
pub(super) fn rebuild_from_sections(
    preamble: &str,
    sections: &std::collections::BTreeMap<u32, String>,
) -> String {
    let mut out = String::new();
    if !preamble.is_empty() {
        out.push_str(preamble);
        out.push_str("\n\n");
    }
    for (page, content) in sections {
        out.push_str(&format!("<!-- PAGE {page} -->\n"));
        out.push_str(content.trim());
        out.push_str("\n\n");
    }
    out.trim().to_string()
}

/// Strip a wrapping code fence (```markdown … ```) if the model emitted one.
#[cfg(test)]
pub(super) fn strip_markdown_fence(text: &str) -> &str {
    let t = text.trim();
    if !t.starts_with("```") {
        return t;
    }
    let Some(first_newline) = t.find('\n') else {
        return t;
    };
    if t[3..first_newline]
        .trim()
        .chars()
        .any(|c| !c.is_ascii_alphanumeric())
    {
        return t;
    }
    let rest = &t[first_newline + 1..];
    match rest.rfind("```") {
        Some(end) if rest[end + 3..].trim().is_empty() => rest[..end].trim(),
        _ => t,
    }
}

/// The shared requirements block for extraction and repair prompts.
pub(super) fn extraction_requirements() -> &'static str {
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
pub(super) fn source_line(attach: bool, prompt_path: &str) -> String {
    if attach {
        "The document is attached to this message as a PDF.".to_string()
    } else {
        format!("Read the PDF file at {prompt_path}.")
    }
}

pub(super) fn llm_initial_ranges(baseline: &[usize]) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut start = 1u32;
    let mut pages = 0usize;
    let mut chars = 0usize;
    for (index, baseline_chars) in baseline.iter().copied().enumerate() {
        if pages > 0
            && (pages >= LLM_CHUNK_MAX_PAGES
                || chars.saturating_add(baseline_chars) > LLM_CHUNK_TARGET_BASELINE_CHARS)
        {
            ranges.push((start, index as u32));
            start = index as u32 + 1;
            pages = 0;
            chars = 0;
        }
        pages += 1;
        chars = chars.saturating_add(baseline_chars);
    }
    if pages > 0 {
        ranges.push((start, baseline.len() as u32));
    }
    ranges
}

pub(super) fn split_ranges(ranges: &[(u32, u32)], max_pages: u32) -> Vec<(u32, u32)> {
    let mut split = Vec::new();
    for &(start, end) in ranges {
        let mut cursor = start;
        while cursor <= end {
            let chunk_end = end.min(cursor.saturating_add(max_pages.saturating_sub(1)));
            split.push((cursor, chunk_end));
            cursor = chunk_end.saturating_add(1);
        }
    }
    split
}

/// Request one bounded, explicit page range. Every requested page marker is
/// mandatory, including blank/figure-only pages, because page completeness is
/// a contract rather than a best-effort quality note.
#[allow(clippy::too_many_arguments)]
pub(super) async fn request_llm_pages(
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
pub(super) async fn run_llm_ranges(
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
        tasks.spawn(request_llm_pages(
            app.clone(),
            path.to_path_buf(),
            prompt_path.to_string(),
            start,
            end,
            attach,
            timeout_secs,
            read_dirs.to_vec(),
            settings.clone(),
        ));
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
                if super::super::provider_error::is_usage_limit_error(&error)
                    || super::super::provider_error::is_non_retryable_error(&error)
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
            tasks.spawn(request_llm_pages(
                app.clone(),
                path.to_path_buf(),
                prompt_path.to_string(),
                start,
                end,
                attach,
                timeout_secs,
                read_dirs.to_vec(),
                settings.clone(),
            ));
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
pub(super) async fn extract_llm(
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
    let staged_path = tokio::task::spawn_blocking(move || stage_pdf_input(&source, &input_root))
        .await
        .map_err(|error| format!("PDF staging task failed: {error}"))??;

    // Completeness baseline (poppler is bundled so this is normally
    // available; the error carries pdftotext's own diagnosis, e.g. an
    // encrypted PDF's "Incorrect password").
    let baseline = {
        let p = staged_path.clone();
        tokio::task::spawn_blocking(move || pdftotext_page_baseline(&p))
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
