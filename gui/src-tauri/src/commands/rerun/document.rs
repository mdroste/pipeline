//! Retained document-bundle restoration and rerun-specific persistence.

use super::files::read_captured_text;

pub(super) struct RestoredDocument {
    pub(super) rerun_bundle_json: Option<String>,
    pub(super) report_bundle: Option<crate::document_bundle::DocumentBundle>,
    pub(super) rerun_document_text: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn restore_document_context(
    app: &crate::emit::EventBus,
    parent: &crate::runs::RunManifest,
    document_text: &str,
    orientation_value: &serde_json::Value,
    orientation_sampling: &crate::pipeline::orient::OrientationSample,
    resumed_extraction: &crate::models::ExtractionResult,
    scoped_source: &crate::pipeline::extract::ScopedSourceContext,
    run_writer: &mut Option<crate::runs::RunWriter>,
) -> Result<RestoredDocument, String> {
    let parent_run_id = &parent.run_id;
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

        let parent_dir = crate::runs::runs_dir()?.join(parent_run_id);
        let raw_path = crate::commands::run_contract::BASE_BUNDLE;
        let mut bundle = if parent
            .artifacts
            .iter()
            .any(|entry| entry.rel_path == raw_path)
            || parent_dir
                .join(raw_path)
                .try_exists()
                .map_err(|e| e.to_string())?
        {
            let json = read_captured_text(parent, raw_path)?;
            let base: crate::document_bundle::DocumentBundle = serde_json::from_str(&json)
                .map_err(|e| format!("Invalid captured document base: {e}"))?;
            base.validate()?;
            Some(base)
        } else {
            None
        };
        if let Some(base) = bundle.as_mut() {
            w.restore_visuals(&parent_dir, parent, base)?;
        } else {
            // Preserve the original extractor's nodes even when the parser is
            // no longer installed. Older bundles lack the extraction-only file.
            if parent_dir
                .join("context/document_bundle.json")
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                let json = read_captured_text(parent, "context/document_bundle.json")?;
                let mut legacy: crate::document_bundle::DocumentBundle =
                    serde_json::from_str(&json)
                        .map_err(|e| format!("Invalid captured document bundle: {e}"))?;
                legacy.validate()?;
                clear_legacy_orientation(&mut legacy);
                w.restore_visuals(&parent_dir, parent, &mut legacy)?;
                bundle = Some(legacy);
            } else {
                let build = crate::commands::reuse_artifacts::build_bundle(
                    resumed_extraction,
                    scoped_source.source_path.as_deref(),
                    w.dir(),
                )?;
                for (rel_path, label, group) in &build.added_artifacts {
                    w.register_existing(rel_path, label, group)?;
                }
                bundle = Some(build.bundle);
            }
        }
        if let Some(bundle) = bundle.as_mut() {
            w.bind_visual_identities(bundle);
            crate::commands::run_contract::save_base_bundle(w, bundle)?;
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

/// A rerun may continue with a documented coverage gap, but a step that
/// explicitly relies on visuals alone cannot execute with its entire input lost.
pub(super) fn require_visual_input(
    config: &crate::pipeline_config::PipelineConfig,
    bundle: Option<&crate::document_bundle::DocumentBundle>,
) -> Result<(), String> {
    use crate::pipeline_config::{ArtifactSelector, PrimaryArtifactPart};
    let Some(bundle) = bundle else { return Ok(()) };
    if !bundle.assets.is_empty() || !bundle.quality.iter().any(|q| q.scope == "visuals") {
        return Ok(());
    }
    for step in config
        .steps
        .iter()
        .filter(|s| s.enabled && s.run_if.is_none())
    {
        if matches!(step.context.include.as_slice(),[ArtifactSelector::Primary{parts}] if parts.as_slice()==[PrimaryArtifactPart::Visuals])
        {
            return Err(format!("Step '{}' needs visual evidence, but none of its retained images could be restored. Start a new review from the source.",step.label));
        }
    }
    Ok(())
}

fn clear_legacy_orientation(bundle: &mut crate::document_bundle::DocumentBundle) {
    let generated: std::collections::HashSet<_> = bundle
        .nodes
        .iter()
        .filter(|n| n.provenance.method == "orientation")
        .map(|n| n.id.clone())
        .collect();
    bundle.nodes.retain(|node| !generated.contains(&node.id));
    bundle
        .links
        .retain(|link| !generated.contains(&link.from_id) && !generated.contains(&link.to_id));
    let mut ambiguous = false;
    for node in &mut bundle.nodes {
        if !node
            .representations
            .iter()
            .any(|r| r.format == "orientation_summary")
        {
            continue;
        }
        // A previous orientation may have filled an empty caption or assigned
        // assets. Keep independent extractor text and independently anchored pages.
        if node.representations.iter().any(|r| {
            r.format == "orientation_summary"
                && r.content["caption_summary"].as_str() == Some(&node.text)
        }) {
            node.text.clear();
        }
        node.representations
            .retain(|r| r.format != "orientation_summary");
        node.asset_ids.clear();
        if node.parent_id.is_none() {
            node.page = None;
        }
        ambiguous = true;
    }
    if ambiguous {
        bundle.quality.push(crate::document_bundle::DocumentQualityNote{severity:"warning".into(),scope:"orientation".into(),
            message:"This older bundle combined extracted and model-assigned figure locations. Review the locations assigned by the new orientation.".into()});
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_legacy_base_keeps_extracted_text_and_drops_the_previous_models_summary() {
        let node = serde_json::json!({"id":"source","kind":"figure","order":1,"text":"captured caption","page":9,
            "representations":[{"format":"orientation_summary","content":{"caption_summary":"obsolete model summary"}}],"provenance":{"origin_id":"o","method":"parser"}});
        let mut generated = node.clone();
        generated["id"] = "model".into();
        generated["provenance"]["method"] = "orientation".into();
        let mut bundle:crate::document_bundle::DocumentBundle=serde_json::from_value(serde_json::json!({
            "schema_version":"1.0","bundle_id":"fixture","source_kind":"pdf","origins":[],"pages":[],"assets":[],"nodes":[node,generated],
            "extraction":{"method":"parser","source_path":"paper.pdf","paper_hash":"fixture"}
        })).unwrap();
        clear_legacy_orientation(&mut bundle);
        assert_eq!(bundle.nodes.len(), 1);
        assert_eq!(bundle.nodes[0].text, "captured caption");
        assert!(bundle.nodes[0].representations.is_empty());
        assert!(bundle.nodes[0].page.is_none());
        assert!(!bundle.quality.is_empty());
    }
}
