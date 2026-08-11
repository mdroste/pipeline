use super::*;

/// Extract text from a paper file or directory.
/// When `app` is provided and the pdf_extractor setting is "llm", uses the
/// configured LLM provider to read and extract the PDF to Markdown.
pub async fn extract(
    app: &crate::emit::EventBus,
    paper_path: &str,
    extraction_cfg: &crate::pipeline_config::ExtractionConfig,
) -> Result<ExtractionResult, String> {
    let path = PathBuf::from(paper_path);

    if !path.exists() {
        return Err(format!("File not found: {paper_path}"));
    }

    // Resolve the effective extraction method: profile override beats global.
    // An explicit "auto" or empty string falls through to whatever the global
    // setting says.
    let settings = crate::settings::load();
    let cfg_method = extraction_cfg.method.trim();
    let effective_method = if cfg_method.is_empty() || cfg_method == "auto" {
        settings.pdf_extractor.clone()
    } else {
        cfg_method.to_string()
    };
    if ext_eq(&path, "pdf") || (path.is_dir() && find_main_tex(&path).is_none()) {
        let source = if cfg_method.is_empty() || cfg_method == "auto" {
            "global setting"
        } else {
            "profile override"
        };
        extraction_log(
            app,
            format!("PDF extractor resolved to {effective_method} ({source})"),
        );
    }
    // All providers can extract PDFs now: direct APIs get the PDF attached
    // to the request, CLIs read it with their multimodal Read tool.
    let use_llm = effective_method == "llm";
    let use_paddle_full = effective_method == "paddleocr-vl-full";

    if use_paddle_full {
        let pdf_path = if path.is_dir() {
            if find_main_tex(&path).is_some() {
                None
            } else {
                find_pdf_in_dir(&path)
            }
        } else if ext_eq(&path, "pdf") {
            Some(path.clone())
        } else {
            None
        };
        if let Some(pdf) = pdf_path {
            let hash = {
                let pdf_for_hash = pdf.clone();
                tokio::task::spawn_blocking(move || compute_hash(&pdf_for_hash))
                    .await
                    .map_err(|error| format!("Hash computation failed: {error}"))??
            };
            // The sidecar's bounded subprocess runner owns the wall-clock
            // timeout. Avoid dropping its blocking task one instant earlier
            // than its process-tree cleanup.
            return extract_paddle_full(app, &pdf, &hash, &settings).await;
        }
    }

    // For LLM PDF extraction, identify the PDF path and run async
    if use_llm {
        let pdf_path = if path.is_dir() {
            // Check for LaTeX first — always prefer native extraction
            if find_main_tex(&path).is_some() {
                None // will fall through to blocking LaTeX extraction
            } else {
                find_pdf_in_dir(&path)
            }
        } else if ext_eq(&path, "pdf") {
            Some(path.clone())
        } else {
            None // .tex or other — fall through to blocking extraction
        };

        if let Some(pdf) = pdf_path {
            let hash = {
                let p = pdf.clone();
                tokio::task::spawn_blocking(move || compute_hash(&p))
                    .await
                    .map_err(|e| format!("Hash computation failed: {e}"))??
            };
            return tokio::time::timeout(
                std::time::Duration::from_secs(settings.pdf_extraction_timeout_secs),
                extract_llm(app, &pdf, &hash),
            )
            .await
            .map_err(|_| {
                format!(
                    "LLM PDF extraction exceeded the {} second document budget",
                    settings.pdf_extraction_timeout_secs
                )
            })?;
        }
    }

    let blocking_method = effective_method.clone();
    let native_app = app.clone();

    // Non-LLM paths: run blocking I/O on a separate thread
    let native_result = tokio::task::spawn_blocking(move || {
        if path.is_dir() {
            if let Some(tex_path) = find_main_tex(&path) {
                // Must be canonical: extract_latex compares it against canonicalized
                // include paths via starts_with. A non-canonical root silently defeats
                // the path-traversal check.
                let root_dir = path.canonicalize().map_err(|e| {
                    format!("Failed to canonicalize project dir {}: {e}", path.display())
                })?;
                let mut warnings = Vec::new();
                let text = extract_latex(&tex_path, 0, &root_dir, &mut warnings)?;
                if text.trim().is_empty() {
                    return Err("LaTeX extraction produced empty output".to_string());
                }
                let hash = content_hash(text.as_bytes());
                return Ok(ExtractionResult {
                    text,
                    method: "latex".to_string(),
                    source_path: tex_path.to_string_lossy().to_string(),
                    paper_hash: hash,
                    quality_notes: warnings,
                });
            }

            if let Some(pdf) = find_pdf_in_dir(&path) {
                return extract_pdf_native(&native_app, &pdf, &blocking_method);
            }

            if let Some(docx) = find_docx_in_dir(&path) {
                return extract_docx(&docx);
            }

            return Err(format!(
                "No .tex, .pdf, or .docx files found in {}",
                path.display()
            ));
        }

        if ext_eq(&path, "tex") {
            // Must be canonical: extract_latex compares it against canonicalized
            // include paths via starts_with. A non-canonical root silently defeats
            // the path-traversal check.
            let parent = path.parent().unwrap_or(Path::new("."));
            let root_dir = parent.canonicalize().map_err(|e| {
                format!(
                    "Failed to canonicalize parent dir {}: {e}",
                    parent.display()
                )
            })?;
            let mut warnings = Vec::new();
            let text = extract_latex(&path, 0, &root_dir, &mut warnings)?;
            if text.trim().is_empty() {
                return Err("LaTeX extraction produced empty output".to_string());
            }
            let hash = content_hash(text.as_bytes());
            Ok(ExtractionResult {
                text,
                method: "latex".to_string(),
                source_path: path.to_string_lossy().to_string(),
                paper_hash: hash,
                quality_notes: warnings,
            })
        } else if ext_eq(&path, "pdf") {
            extract_pdf_native(&native_app, &path, &blocking_method)
        } else if ext_eq(&path, "docx") {
            extract_docx(&path)
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("(none)");
            Err(format!("Unsupported file type: .{ext}"))
        }
    })
    .await
    .map_err(|e| format!("Extraction task failed: {e}"))?;

    native_result
}
