//! Managed full-parser invocation and lifetime.
use super::*;

pub(crate) async fn extract_paddle_full(
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
    let (paths, engine_lease) = crate::commands::await_or_cancel(
        tokio::task::spawn_blocking(crate::engines::paddle_full_parser_lease),
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
    let sidecar_lease = engine_lease.clone();
    let sidecar = tokio::task::spawn_blocking(move || {
        let _lease = sidecar_lease;
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
