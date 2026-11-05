//! Immutable run manifest, artifact and history records.
use super::MAX_MANIFEST_ARTIFACTS;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    /// Path relative to the run directory, forward slashes.
    pub rel_path: String,
    /// Human-readable label for the explorer tree.
    pub label: String,
    /// Detected kind: markdown | code | json | csv | image | pdf | text | binary.
    pub kind: String,
    pub bytes: u64,
    /// First 16 hex chars of the SHA-256, matching the paper-hash style.
    pub sha256: String,
    /// Grouping hint for the explorer: report | product | context |
    /// agent_response | step.
    pub group: String,
}

/// Constant-size index for the many homogeneous page images in a completed
/// run. The files remain individually addressable for visual inspection and
/// re-runs, but do not each occupy a full artifact-manifest record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageArtifactIndex {
    pub count: u32,
    pub digit_width: u8,
    pub extension: String,
    pub total_bytes: u64,
}

/// Durable identity of the selected subject, independent of whether it was a
/// document, LaTeX project, source tree, or input-free workflow.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputIdentity {
    /// Canonical selection spelling plus its explicit interpretation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub selection_key: String,
    /// Hash produced by the input adapter (document text or bounded tree
    /// fingerprint). Promoted from the historical run-id/report-only field.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content_hash: String,
    /// Stable across re-runs. Fresh selections derive it from selection_key;
    /// a re-run inherits it even if its source is later moved or renamed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lineage_id: String,
}

impl PageArtifactIndex {
    pub(super) fn rel_path(&self, page: u32) -> Result<String, String> {
        if self.count == 0 || self.count as usize > MAX_MANIFEST_ARTIFACTS {
            return Err("Invalid page artifact count".to_string());
        }
        if page == 0 || page > self.count {
            return Err(format!(
                "Page {page} is outside this run's 1-{} page range",
                self.count
            ));
        }
        if self.digit_width == 0 || self.digit_width > 8 {
            return Err("Invalid page artifact index".to_string());
        }
        if !matches!(self.extension.as_str(), "jpg" | "jpeg" | "png") {
            return Err("Invalid page artifact format".to_string());
        }
        let width = self.digit_width as usize;
        Ok(format!(
            "artifacts/pages/page-{page:0width$}.{}",
            self.extension
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    /// Version of the durable run-artifact layout. Manifests written before
    /// this field was introduced deserialize as version 0.
    #[serde(default)]
    pub artifact_schema_version: u32,
    pub run_id: String,
    pub created: String,
    pub input_path: String,
    pub input_mode: String,
    /// Explicit run-time meaning assigned to the primary selection. Empty on
    /// manifests written before input interpretations were introduced.
    #[serde(default)]
    pub input_interpretation: String,
    #[serde(default)]
    pub input_identity: InputIdentity,
    pub profile_id: String,
    pub profile_name: String,
    /// Where the executable workflow came from (`profile:<id>`, `stdin`, or a
    /// file path). Empty on manifests written before portable workflows.
    #[serde(default)]
    pub workflow_source: String,
    /// SHA-256 of the normalized portable workflow retained as an artifact.
    #[serde(default)]
    pub workflow_fingerprint: String,
    /// SHA-256 of the modular specialist catalog used to resolve catalog-backed
    /// schemas. Empty for workflows that do not use Automatic Paper Review.
    #[serde(default)]
    pub specialist_catalog_revision: String,
    pub provider: String,
    pub artifacts: Vec<ArtifactEntry>,
    /// Completed runs compact homogeneous page records into this descriptor.
    /// Older and interrupted runs keep page entries in `artifacts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_artifacts: Option<PageArtifactIndex>,
    // ── Run-level metadata (all defaulted so pre-1.1 manifests still load) ──
    /// Outcome: "done" | "partial" (some steps failed) | "failed" |
    /// "cancelled" | "interrupted" (process ended before finalization).
    #[serde(default)]
    pub status: String,
    /// Wall-clock time the whole run took, in seconds.
    #[serde(default)]
    pub duration_secs: u64,
    /// Total token usage across the run (steps, orientation, merge, extraction).
    #[serde(default)]
    pub usage: crate::pipeline::logging::CallUsage,
    /// Number of steps that produced output.
    #[serde(default)]
    pub step_count: u32,
    /// Labels of steps that failed, for the history list.
    #[serde(default)]
    pub failed_steps: Vec<String>,
    /// User-assigned title (empty = fall back to the input name).
    #[serde(default)]
    pub title: String,
    /// User-assigned tags for filtering the history list.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Run-time variable values the run was launched with.
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
    /// Named extra inputs captured inside the run, keyed by profile slot. Values
    /// are paths relative to the run directory.
    #[serde(default)]
    pub extra_inputs: std::collections::HashMap<String, String>,
    /// Original named-input file/folder paths. These let an explicit
    /// `named_input.source` selector retain its meaning on a re-run.
    #[serde(default)]
    pub extra_input_sources: std::collections::HashMap<String, String>,
    /// The run this one was re-run from, if any (resume / partial re-run).
    #[serde(default)]
    pub parent_run_id: Option<String>,
}

/// Metadata recorded when a run finishes. Grouped into a struct so `finish`
/// doesn't take a dozen positional arguments.
#[derive(Debug, Clone, Default)]
pub struct RunFinishMeta {
    pub input_path: String,
    pub input_mode: String,
    pub input_interpretation: String,
    pub input_identity: InputIdentity,
    pub profile_id: String,
    pub profile_name: String,
    pub workflow_source: String,
    pub workflow_fingerprint: String,
    pub specialist_catalog_revision: String,
    pub provider: String,
    pub status: String,
    pub duration_secs: u64,
    pub usage: crate::pipeline::logging::CallUsage,
    pub step_count: u32,
    pub failed_steps: Vec<String>,
    pub variables: std::collections::HashMap<String, String>,
    pub extra_inputs: std::collections::HashMap<String, String>,
    pub extra_input_sources: std::collections::HashMap<String, String>,
    pub parent_run_id: Option<String>,
}

/// A lightweight row for the run-history list: everything except the (possibly
/// large) artifact index.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub created: String,
    /// Basename of the input path, or "(no input)" for input-free runs.
    pub input_name: String,
    pub input_path: String,
    pub input_mode: String,
    pub input_interpretation: String,
    pub input_identity: InputIdentity,
    pub profile_id: String,
    pub profile_name: String,
    pub workflow_source: String,
    pub workflow_fingerprint: String,
    pub specialist_catalog_revision: String,
    pub provider: String,
    pub status: String,
    pub duration_secs: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub model_round_trips: u64,
    pub tool_calls: crate::models::ToolCallCounts,
    pub step_count: u32,
    pub artifact_count: u32,
    pub failed_steps: Vec<String>,
    /// The run has a structured report plus captured document and can continue
    /// from its last durable step through the History page.
    pub resumable: bool,
    pub title: String,
    pub tags: Vec<String>,
}

/// Total number of runs on disk and the bytes they occupy.
#[derive(Debug, Clone, Serialize)]
pub struct RunsDiskUsage {
    pub count: u32,
    pub bytes: u64,
}

/// Exact retention effect shown before the user confirms a manual purge.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RunPurgePreview {
    pub delete_count: u32,
    pub delete_bytes: u64,
    pub remaining_count: u32,
    pub remaining_bytes: u64,
    /// Opaque digest of the exact deletion set and totals shown to the user.
    /// Manual purge must present this token again so a stale confirmation can
    /// never authorize a newly calculated set of runs.
    pub preview_token: String,
}

pub(super) fn input_basename(input_path: &str) -> String {
    if input_path.trim().is_empty() {
        return "(no input)".to_string();
    }
    Path::new(input_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| input_path.to_string())
}

impl RunManifest {
    pub(super) fn to_summary(&self) -> RunSummary {
        RunSummary {
            run_id: self.run_id.clone(),
            created: self.created.clone(),
            input_name: input_basename(&self.input_path),
            input_path: self.input_path.clone(),
            input_mode: self.input_mode.clone(),
            input_interpretation: self.input_interpretation.clone(),
            input_identity: self.input_identity.clone(),
            profile_id: self.profile_id.clone(),
            profile_name: self.profile_name.clone(),
            workflow_source: self.workflow_source.clone(),
            workflow_fingerprint: self.workflow_fingerprint.clone(),
            specialist_catalog_revision: self.specialist_catalog_revision.clone(),
            provider: self.provider.clone(),
            status: if self.status.is_empty() {
                "done".to_string()
            } else {
                self.status.clone()
            },
            duration_secs: self.duration_secs,
            input_tokens: self.usage.input_tokens,
            output_tokens: self.usage.output_tokens,
            cached_input_tokens: self.usage.cached_input_tokens,
            cache_write_input_tokens: self.usage.cache_write_input_tokens,
            model_round_trips: self.usage.model_round_trips,
            tool_calls: self.usage.tool_calls,
            step_count: self.step_count,
            artifact_count: self.artifacts.len() as u32
                + self
                    .page_artifacts
                    .as_ref()
                    .map(|pages| pages.count)
                    .unwrap_or(0),
            failed_steps: self.failed_steps.clone(),
            resumable: false,
            title: self.title.clone(),
            tags: self.tags.clone(),
        }
    }
}

/// What `read_artifact` returns to the frontend.
#[derive(Debug, Serialize)]
pub struct ArtifactContent {
    pub kind: String,
    pub bytes: u64,
    /// UTF-8 text content (lossy) for text-like kinds; None for binary.
    pub text: Option<String>,
    /// Base64 content for images small enough to inline.
    pub base64: Option<String>,
    /// Set when text was cut at MAX_TEXT_BYTES.
    pub truncated: bool,
    /// Absolute path, for "open externally".
    pub abs_path: String,
}

/// One PDF page rendered to an inline-safe JPEG for the artifact explorer.
#[derive(Debug, Serialize)]
pub struct PrefetchedPdfArtifactPage {
    pub page: u32,
    pub has_previous: bool,
    pub has_next: bool,
    pub base64: String,
}

#[derive(Debug, Serialize)]
pub struct PdfArtifactPage {
    pub page: u32,
    pub has_previous: bool,
    pub has_next: bool,
    pub base64: String,
    pub prefetched_next: Option<PrefetchedPdfArtifactPage>,
}
