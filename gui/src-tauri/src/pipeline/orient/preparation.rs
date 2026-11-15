//! Model-independent document inventory and the compatibility survey view.
//! Captured structure is never rewritten by reviewer selection. Stock inventory
//! fields are supplied locally; deliberately customized schema fields still go
//! to the model. Empty inventories mean unobserved, not absent from the source.

use crate::{document_bundle::DocumentBundle, models::ExtractionResult};
use serde_json::{json, Map, Value};

pub const PREPARATION_PATH: &str = "context/document_preparation.json";
pub const REVIEW_PLAN_PATH: &str = "context/review_plan.json";
pub const REGISTRY_PATH: &str = "context/reviewer_registry.json";

pub(crate) fn document_preparation_snapshot(
    extraction: &ExtractionResult,
    bundle: Option<&DocumentBundle>,
    schema: Option<&Value>,
    supporting: &[SupportingInput],
) -> Result<Option<String>, String> {
    let Some(schema) =
        schema.filter(|schema| super::document_type::expected_document(schema).is_some())
    else {
        return Ok(None);
    };
    crate::auto_review::validate_schema_settings(schema)?;
    let resolved = crate::auto_review::resolve_schema_catalogs(schema)?;
    let preparation = DocumentPreparation::new(extraction, bundle, &resolved, supporting);
    serde_json::to_string_pretty(&preparation.snapshot)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub struct SupportingInput {
    pub key: String,
    pub text: String,
}

pub struct PreparedReview {
    pub orientation: Value,
    pub index: Option<String>,
    pub preparation: Option<String>,
    pub registry: Option<String>,
}

impl PreparedReview {
    pub fn persist_preparation(&self, writer: &mut crate::runs::RunWriter) -> Result<(), String> {
        if let Some(content) = &self.preparation {
            writer.add_text(PREPARATION_PATH, "Document preparation", "context", content)?;
        }
        Ok(())
    }

    /// Save the plan separately; the initial run already saved preparation
    /// before its first model call. Rebuilds persist their preparation explicitly.
    pub fn persist(&self, writer: &mut crate::runs::RunWriter) -> Result<(), String> {
        for (path, label, content) in [
            (
                super::ORIENTATION_INDEX_PATH,
                "Document index",
                self.index.as_deref(),
            ),
            (
                REGISTRY_PATH,
                "Reviewer registry snapshot",
                self.registry.as_deref(),
            ),
        ] {
            if let Some(content) = content {
                writer.add_text(path, label, "context", content)?;
            }
        }
        if let Some(plan) = self.orientation.get("review_plan") {
            writer.add_text(
                REVIEW_PLAN_PATH,
                "Review plan",
                "context",
                &serde_json::to_string_pretty(plan).map_err(|e| e.to_string())?,
            )?;
        }
        Ok(())
    }
}

pub(super) struct DocumentPreparation {
    fields: Map<String, Value>,
    pub provider_schema: Value,
    pub snapshot: Value,
}

impl DocumentPreparation {
    pub fn new(
        extraction: &ExtractionResult,
        bundle: Option<&DocumentBundle>,
        schema: &Value,
        supporting: &[SupportingInput],
    ) -> Self {
        let stock = crate::orientation_contract::paper_schema();
        let mut provider_schema = schema.clone();
        let mut fields = Map::new();
        for key in [
            "sections",
            "formal_results",
            "tables_figures",
            "notation",
            "key_references",
            "extraction_quality_notes",
        ] {
            if schema["properties"][key] == stock["properties"][key] {
                fields.insert(key.into(), json!([]));
                provider_schema["properties"]
                    .as_object_mut()
                    .expect("validated schema")
                    .remove(key);
                provider_schema["required"]
                    .as_array_mut()
                    .expect("validated schema")
                    .retain(|value| value != key);
            }
        }
        let mut truncated = false;
        if let Some(bundle) = bundle {
            let mut nodes = bundle
                .nodes
                .iter()
                .filter(|node| node.provenance.method != "orientation")
                .collect::<Vec<_>>();
            nodes.sort_by_key(|node| node.order);
            for node in nodes {
                let page = node.page.unwrap_or(0);
                let number = node.number.as_deref().unwrap_or_default();
                let text = if node.text.is_empty() {
                    node.label.as_deref().unwrap_or_default()
                } else {
                    &node.text
                };
                let text = &text[..super::sampling::char_boundary_before(text, 700)];
                let (field, entry) = match node.kind.as_str() {
                    "section" => (
                        "sections",
                        json!({"number":number,"title":text,"page_start":page,"page_end":0}),
                    ),
                    "theorem" | "proposition" | "lemma" | "corollary" | "definition"
                    | "assumption" => (
                        "formal_results",
                        json!({"kind":node.kind,"number":number,"page":page,"summary":text,"proof_location":""}),
                    ),
                    "table" | "figure" => (
                        "tables_figures",
                        json!({"kind":node.kind,"number":number,"page":page,"caption_summary":text,"what_it_shows":""}),
                    ),
                    _ => continue,
                };
                if let Some(items) = fields.get_mut(field).and_then(Value::as_array_mut) {
                    let limit = stock["properties"][field]["maxItems"]
                        .as_u64()
                        .unwrap_or(100) as usize;
                    if items.len() < limit {
                        items.push(entry);
                    } else {
                        truncated = true;
                    }
                }
            }
        }
        if let Some(notes) = fields.get_mut("extraction_quality_notes") {
            let mut quality =
                super::bounded_host_notes(&extraction.quality_notes, super::MAX_HOST_QUALITY_NOTES);
            if truncated {
                quality.push(
                    "The observed document inventory was shortened to its item limit.".into(),
                );
            }
            if super::orientation_sample(&extraction.text).truncated {
                quality.push(super::INDEXED_ORIENTATION_NOTICE.into());
            }
            *notes = super::quality_note_values(&quality).into();
        }
        let snapshot = json!({"schema_version":1,"paper_hash":extraction.paper_hash,
            "bundle_id":bundle.map(|bundle|&bundle.bundle_id),"extraction_method":extraction.method,
            "inventory_source":"captured structure; not an exhaustive semantic inventory",
            "fields":fields,"supporting_inputs":supporting.iter().map(|input|{
                use sha2::{Digest,Sha256};
                json!({"key":input.key,"text_sha256":format!("{:x}",Sha256::digest(input.text.as_bytes()))})
            }).collect::<Vec<_>>()});
        Self {
            fields,
            provider_schema,
            snapshot,
        }
    }

    pub fn complete(&self, mut orientation: Value) -> Result<Value, String> {
        let object = orientation
            .as_object_mut()
            .ok_or("The review plan must be an object.")?;
        for (key, value) in &self.fields {
            // Providers cannot replace the document inventory through an
            // undeclared extra output property.
            object.insert(key.clone(), value.clone());
        }
        Ok(orientation)
    }
}
