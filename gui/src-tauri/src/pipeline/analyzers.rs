//! Deterministic host analyzers for Reviews. Each analyzer reads run inputs
//! and products and records what it checked; none calls a model or edits
//! model text.
//!
//! - `quotes` verifies quoted evidence after every Review.
//! - `references` and `replication` run as analyzer workflow steps, listed in
//!   `registry`.

pub mod fold;
pub mod quotes;
pub mod references;
pub mod registry;
pub mod replication;

use crate::pipeline_config::AnalyzerSpec;
use serde_json::Value;

/// Run inputs available to an analyzer step.
pub struct AnalyzerContext<'a> {
    pub document_bundle_path: &'a str,
    pub paper_text_path: &'a str,
    pub source_path: &'a str,
}

fn read_bounded(path: &str, max: u64) -> Option<Vec<u8>> {
    if path.trim().is_empty() {
        return None;
    }
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > max {
        return None;
    }
    std::fs::read(path).ok()
}

/// Runs one analyzer step and returns its JSON product.
pub async fn run(spec: &AnalyzerSpec, context: &AnalyzerContext<'_>) -> Result<Value, String> {
    let info = registry::find(&spec.id, spec.version)
        .ok_or_else(|| format!("Unknown analyzer '{}' version {}", spec.id, spec.version))?;
    match info.id {
        registry::REFERENCE_AUDIT => {
            let bundle = read_bounded(
                context.document_bundle_path,
                crate::document_bundle::MAX_DOCUMENT_BUNDLE_BYTES as u64,
            )
            .and_then(|bytes| {
                serde_json::from_slice::<crate::document_bundle::DocumentBundle>(&bytes).ok()
            });
            let text = read_bounded(context.paper_text_path, 16 * 1024 * 1024)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            references::run(bundle.as_ref(), &text, context.source_path, &spec.options).await
        }
        registry::REPLICATION_STATIC => {
            let root = std::path::PathBuf::from(context.source_path);
            let options = spec.options.clone();
            tokio::task::spawn_blocking(move || replication::run(&root, &options))
                .await
                .map_err(|error| format!("Replication package check stopped: {error}"))?
        }
        other => Err(format!("Analyzer '{other}' has no implementation")),
    }
}
