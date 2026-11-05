//! Page render and full-parser figure collection for a fresh run.

pub(super) async fn collect_visual_artifacts(
    app: &crate::emit::EventBus,
    input_mode: &str,
    captured_visual_pdf: Option<std::path::PathBuf>,
    extraction: &crate::models::ExtractionResult,
    run_writer: &mut Option<crate::runs::RunWriter>,
) {
    // Render page images for PDF inputs, and for LaTeX projects that have a
    // compiled companion PDF. The semantic LaTeX source remains primary, but
    // the page renders preserve the visual evidence needed to inspect figures,
    // tables, equation layout, and extraction quality.
    let visual_pdf = captured_visual_pdf;
    if input_mode == "document" {
        if let (Some(pdf), Some(w)) = (visual_pdf, run_writer.as_mut()) {
            let out_dir = w.dir().join("artifacts").join("pages");
            let rendered = crate::run_scope::spawn_blocking(move || {
                crate::pipeline::extract::render_pdf_pages(
                    &pdf,
                    &out_dir,
                    crate::pipeline::extract::MAX_RENDERED_PDF_PAGES,
                )
            })
            .await;
            match rendered {
                Ok(Ok(rendered)) => {
                    let names = rendered.names;
                    let count = names.len();
                    for name in &names {
                        let page_num = std::path::Path::new(name)
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .unwrap_or(name)
                            .rsplit('-')
                            .next()
                            .and_then(|n| n.parse::<u32>().ok());
                        let label = match page_num {
                            Some(n) => format!("Page {n}"),
                            None => name.clone(),
                        };
                        if let Err(e) =
                            w.register_existing(&format!("artifacts/pages/{name}"), &label, "pages")
                        {
                            let _ = app.emit_event(
                                "pipeline:log",
                                serde_json::json!({
                                    "line": format!("WARNING: {e}")
                                }),
                            );
                        }
                    }
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("Rendered {count} page images into the run artifacts")
                        }),
                    );
                    if rendered.truncated {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!(
                                    "WARNING: page image rendering reached the {}-page cap; a longer PDF is truncated in the artifact view",
                                    crate::pipeline::extract::MAX_RENDERED_PDF_PAGES
                                )
                            }),
                        );
                    }
                }
                Ok(Err(e)) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: page image rendering skipped: {e}")
                        }),
                    );
                }
                Err(e) => {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: page image rendering task failed: {e}")
                        }),
                    );
                }
            }
        }
    }

    if extraction.method == "paddleocr-vl-full" {
        if let Some(w) = run_writer.as_mut() {
            let inventory =
                match crate::pipeline::extract::paddle_full_image_inventory(&extraction.paper_hash)
                {
                    Ok(inventory) => inventory,
                    Err(error) => {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!(
                                    "WARNING: full-parser image discovery stopped: {error}"
                                )
                            }),
                        );
                        Default::default()
                    }
                };
            if !inventory.artifacts.is_empty() {
                let figures_dir = w.dir().join("artifacts").join("figures");
                if let Err(error) = std::fs::create_dir_all(&figures_dir) {
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!(
                                "WARNING: could not create full-parser figures directory: {error}"
                            )
                        }),
                    );
                } else {
                    let mut copied = 0usize;
                    for artifact in &inventory.artifacts {
                        let Some(name) = artifact
                            .source_path
                            .file_name()
                            .and_then(|name| name.to_str())
                        else {
                            continue;
                        };
                        if std::fs::copy(&artifact.source_path, figures_dir.join(name)).is_ok()
                            && w.register_existing(
                                &format!("artifacts/figures/{name}"),
                                &artifact.display_name,
                                "figures",
                            )
                            .is_ok()
                        {
                            copied += 1;
                        }
                    }
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!(
                                "Collected {copied} image artifact(s) from PaddleOCR-VL Full Parser{}",
                                if inventory.disambiguated_labels == 0 {
                                    String::new()
                                } else {
                                    format!(
                                        " (disambiguated {} repeated figure/table label(s))",
                                        inventory.disambiguated_labels
                                    )
                                }
                            )
                        }),
                    );
                }
            }
        }
    }
}
