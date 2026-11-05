//! Retained document-bundle restoration and rerun-specific persistence.

use super::files::read_run_file;

pub(super) struct RestoredDocument {
    pub(super) rerun_bundle_json: Option<String>,
    pub(super) report_bundle: Option<crate::document_bundle::DocumentBundle>,
    pub(super) rerun_document_text: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn restore_document_context(
    app: &crate::emit::EventBus,
    parent_run_id: &str,
    document_text: &str,
    orientation_value: &serde_json::Value,
    orientation_sampling: &crate::pipeline::orient::OrientationSample,
    resumed_extraction: &crate::models::ExtractionResult,
    scoped_source: &crate::pipeline::extract::ScopedSourceContext,
    run_writer: &mut Option<crate::runs::RunWriter>,
) -> Result<RestoredDocument, String> {
    let mut rerun_bundle_json = None;
    let mut report_bundle = None;
    let mut rerun_document_text = document_text.to_owned();
    if let Some(w) = run_writer.as_mut() {
        w.add_text(
            crate::runs::DOCUMENT_TEXT_PATH,
            "Readable document",
            "document",
            document_text,
        )
        .map_err(|error| format!("Could not save the restored document: {error}"))?;
        let orient_json = serde_json::to_string_pretty(orientation_value)
            .map_err(|error| format!("Could not serialize the orientation map: {error}"))?;
        w.add_text(
            "context/orientation.json",
            "Orientation map",
            "context",
            &orient_json,
        )
        .map_err(|error| format!("Could not save the orientation map: {error}"))?;
        let sampling_json = serde_json::to_string_pretty(orientation_sampling)
            .map_err(|error| format!("Could not serialize orientation coverage: {error}"))?;
        w.add_text(
            "context/orientation_sampling.json",
            "Orientation sampling coverage",
            "context",
            &sampling_json,
        )
        .map_err(|error| format!("Could not save orientation sampling coverage: {error}"))?;

        // Prefer the parent's canonical bundle so a partial re-run keeps the
        // exact document model it was based on. Copy its visual assets into
        // the new run so the bundle remains self-contained. Older runs fall
        // back to a bundle rebuilt from their captured document.
        let mut bundle = read_run_file(parent_run_id, "context/document_bundle.json")
            .ok()
            .and_then(|json| {
                serde_json::from_str::<crate::document_bundle::DocumentBundle>(&json).ok()
            })
            .filter(|bundle| bundle.validate().is_ok());
        if let Some(parent_bundle) = bundle.as_ref() {
            if let Ok(parent_root) = crate::runs::runs_dir() {
                let parent_root = parent_root.join(parent_run_id);
                for asset in &parent_bundle.assets {
                    let source = parent_root.join(&asset.rel_path);
                    let destination = w.dir().join(&asset.rel_path);
                    if !source.is_file() {
                        continue;
                    }
                    if let Some(directory) = destination.parent() {
                        let _ = std::fs::create_dir_all(directory);
                    }
                    if std::fs::copy(&source, &destination).is_ok() {
                        let group = if asset.kind == "page" {
                            "pages"
                        } else {
                            "figures"
                        };
                        let _ = w.register_existing(&asset.rel_path, &asset.label, group);
                    }
                }
            }
        } else if let Ok(build) = crate::commands::reuse_artifacts::build_bundle(
            resumed_extraction,
            scoped_source.source_path.as_deref(),
            w.dir(),
        ) {
            for (rel_path, label, group) in &build.added_artifacts {
                let _ = w.register_existing(rel_path, label, group);
            }
            bundle = Some(build.bundle);
        }
        if let Some(bundle) = bundle.as_mut() {
            bundle.enrich_from_orientation(orientation_value);
            if let Err(error) = bundle.validate() {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!("WARNING: document bundle validation failed: {error}")
                    }),
                );
            } else {
                let markdown = bundle.to_markdown_with_text(document_text);
                rerun_document_text = markdown.clone();
                match bundle
                    .to_json_pretty()
                    .and_then(|json| bundle.to_jsonl().map(|jsonl| (json, jsonl)))
                {
                    Ok((json, jsonl)) => {
                        w.add_text(
                            "context/document_bundle.json",
                            "Document bundle",
                            "document",
                            &json,
                        )
                        .map_err(|error| format!("Could not save the document bundle: {error}"))?;
                        w.add_text(
                            "context/blocks.jsonl",
                            "Document blocks",
                            "document",
                            &jsonl,
                        )
                        .map_err(|error| format!("Could not save document blocks: {error}"))?;
                        rerun_bundle_json = Some(json);
                        report_bundle = Some(bundle.clone());
                    }
                    Err(error) => {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!(
                                    "WARNING: structured document context was omitted: {error}"
                                )
                            }),
                        );
                    }
                }
            }
        }
    }

    Ok(RestoredDocument {
        rerun_bundle_json,
        report_bundle,
        rerun_document_text,
    })
}
