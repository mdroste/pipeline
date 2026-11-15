//! Generated slide PDFs, page images, and full-parser figures for a fresh run.

pub(super) async fn collect_visual_artifacts(
    app: &crate::emit::EventBus,
    input_mode: &str,
    captured_visual_pdf: Option<std::path::PathBuf>,
    extraction: &mut crate::models::ExtractionResult,
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
                        extraction.quality_notes.push(format!("Visual page images stop at page {}; inspect the retained PDF for later pages.", crate::pipeline::extract::MAX_RENDERED_PDF_PAGES));
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
                    extraction.quality_notes.push(format!("Page images could not be rendered: {e}. Use the retained PDF for visual evidence."));
                    let _ = app.emit_event(
                        "pipeline:log",
                        serde_json::json!({
                            "line": format!("WARNING: page image rendering skipped: {e}")
                        }),
                    );
                }
                Err(e) => {
                    extraction.quality_notes.push(format!("Page image rendering task failed: {e}. Use the retained PDF for visual evidence."));
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

/// Only a successfully produced PDF supersedes the native extraction's visual
/// limitation. A failed attempt must not imply that a stale companion was used.
pub(super) fn apply_slide_render_note(
    extraction: &mut crate::models::ExtractionResult,
    rendered: &crate::pipeline::extract::slides::SlideRender,
) {
    if rendered.pdf.is_some() {
        extraction.quality_notes.retain(|note| {
            !note.starts_with("PowerPoint text and speaker notes extracted")
                && !note.starts_with("Beamer source review:")
        });
    }
    extraction.quality_notes.push(rendered.note.clone());
}

pub(super) fn retain_slide_render(
    writer: &mut crate::runs::RunWriter,
    rendered: &crate::pipeline::extract::slides::SlideRender,
) -> Result<(), String> {
    use crate::pipeline::extract::slides::{SLIDE_LOG_PATH, SLIDE_PDF_PATH};
    for (source, relative, label, group) in [
        (
            Some(&rendered.log),
            SLIDE_LOG_PATH,
            "Slide rendering log",
            "context",
        ),
        (
            rendered.pdf.as_ref(),
            SLIDE_PDF_PATH,
            "Rendered slide deck",
            "document",
        ),
    ] {
        if let Some(source) = source {
            super::super::reuse_artifacts::copy_tree(source, &writer.dir().join(relative))?;
            writer.register_existing(relative, label, group)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::extract::slides::{SlideRender, SLIDE_LOG_PATH, SLIDE_PDF_PATH};

    #[test]
    fn rendered_pdf_and_log_are_retained_and_linked_in_the_bundle() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("talk.tex");
        std::fs::write(&source, "\\documentclass{beamer}").unwrap();
        let pdf = temp.path().join("generated.pdf");
        let log = temp.path().join("render.log");
        std::fs::write(&pdf, b"%PDF-1.7\nretention fixture").unwrap();
        std::fs::write(&log, "compiled").unwrap();
        let rendered = SlideRender {
            pdf: Some(pdf),
            log,
            note: "Rendered visual evidence is available".into(),
        };
        let mut extraction = crate::models::ExtractionResult {
            text: "\\documentclass{beamer}".into(),
            method: "latex".into(),
            source_path: source.to_string_lossy().into_owned(),
            paper_hash: "test".into(),
            quality_notes: vec![
                "Beamer source review: TeX is not compiled".into(),
                "A figure was omitted".into(),
            ],
        };
        apply_slide_render_note(&mut extraction, &rendered);
        assert_eq!(
            extraction.quality_notes,
            vec![
                "A figure was omitted",
                "Rendered visual evidence is available"
            ]
        );
        let mut writer = crate::runs::RunWriter::create_in(temp.path(), "slides").unwrap();
        retain_slide_render(&mut writer, &rendered).unwrap();
        assert!(writer.dir().join(SLIDE_PDF_PATH).is_file());
        assert!(writer.dir().join(SLIDE_LOG_PATH).is_file());
        let build = crate::document_bundle::build(&extraction, writer.dir()).unwrap();
        assert!(build
            .bundle
            .assets
            .iter()
            .any(|asset| asset.rel_path == SLIDE_PDF_PATH && asset.kind == "document"));
        assert!(build
            .bundle
            .origins
            .iter()
            .any(|origin| origin.id == "origin-rendered-slides" && origin.sha256.is_some()));
        let failed = SlideRender {
            pdf: None,
            log: rendered.log,
            note: "Rendering failed; source only".into(),
        };
        extraction.quality_notes = vec!["Beamer source review: TeX is not compiled".into()];
        apply_slide_render_note(&mut extraction, &failed);
        assert_eq!(extraction.quality_notes.len(), 2);
    }
}
