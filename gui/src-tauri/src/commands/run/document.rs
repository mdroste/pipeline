//! Document-bundle construction and persistence before review planning.

pub(super) fn build_document_bundle(
    app: &crate::emit::EventBus,
    extraction: &crate::models::ExtractionResult,
    captured_document_path: &std::path::Path,
    run_writer: &mut Option<crate::runs::RunWriter>,
) -> Option<crate::document_bundle::DocumentBundle> {
    // Build the durable, source-neutral document model after visual artifacts
    // have been collected. Persist it before model planning so routing cannot
    // rewrite the captured source structure.
    let mut document_bundle = None;
    if let Some(w) = run_writer.as_mut() {
        match crate::commands::reuse_artifacts::build_bundle(
            extraction,
            Some(captured_document_path),
            w.dir(),
        ) {
            Ok(mut build) => {
                for (rel_path, label, group) in &build.added_artifacts {
                    if let Err(error) = w.register_existing(rel_path, label, group) {
                        let _ = app.emit_event(
                            "pipeline:log",
                            serde_json::json!({
                                "line": format!("WARNING: {error}")
                            }),
                        );
                    }
                }
                w.bind_visual_identities(&mut build.bundle);
                document_bundle = Some(build.bundle);
            }
            Err(error) => {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!(
                            "WARNING: could not build the structured document bundle: {error}"
                        )
                    }),
                );
            }
        }
    }

    document_bundle
}

pub(super) fn persist_document_bundle(
    app: &crate::emit::EventBus,
    document_bundle: &Option<crate::document_bundle::DocumentBundle>,
    extraction: &crate::models::ExtractionResult,
    run_writer: &mut Option<crate::runs::RunWriter>,
) -> Result<(Option<String>, Option<String>), String> {
    // Model annotations remain in the document overview. The bundle keeps
    // the extraction and its observed structure independently of the plan.
    let mut bundle_json = None;
    let mut bundle_markdown = None;
    if let Some(bundle) = document_bundle.as_ref() {
        crate::commands::run_contract::save_base_bundle(
            run_writer.as_mut().ok_or("Missing document workspace")?,
            bundle,
        )?;
        if let Err(error) = bundle.validate() {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: document bundle validation failed: {error}")
                }),
            );
        } else {
            let markdown = bundle.to_markdown_with_text(&extraction.text);
            bundle_markdown = Some(markdown);
            match bundle
                .to_json_pretty()
                .and_then(|json| bundle.to_jsonl().map(|jsonl| (json, jsonl)))
            {
                Ok((json, jsonl)) => {
                    let writer = run_writer.as_mut().ok_or_else(|| {
                        "Durable run workspace disappeared before document bundling".to_string()
                    })?;
                    writer
                        .add_text(
                            "context/document_bundle.json",
                            "Document bundle",
                            "document",
                            &json,
                        )
                        .map_err(|error| format!("Could not save the document bundle: {error}"))?;
                    writer
                        .add_text(
                            "context/blocks.jsonl",
                            "Document blocks",
                            "document",
                            &jsonl,
                        )
                        .map_err(|error| format!("Could not save document blocks: {error}"))?;
                    bundle_json = Some(json);
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

    Ok((bundle_json, bundle_markdown))
}
