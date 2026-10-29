//! Selective exchange wire records and format bounds.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const EXCHANGE_FORMAT: &str = "pipeline-project-exchange";
pub const EXCHANGE_FORMAT_VERSION: u32 = 1;
pub(super) const MAX_OBJECTS: usize = 5000;
pub(super) const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(super) const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
pub(super) const MAX_BLOB_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const MAX_ENTRIES: usize = 20_000;
pub(super) const CAPTURED_OUTPUT_LIMIT: usize = 64 * 1024;
pub const RECORD_KINDS: [&str; 15] = [
    "task",
    "anchor",
    "checkpoint",
    "application",
    "build",
    "response",
    "experiment",
    "specification",
    "series",
    "binding",
    "bibliography",
    "literature",
    "theory",
    "check",
    "direction",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExchangeSelection {
    #[serde(default)]
    pub include_notes: bool,
    #[serde(default)]
    pub record_kinds: Vec<String>,
    #[serde(default)]
    pub paper_ids: Vec<String>,
    #[serde(default)]
    pub execution_ids: Vec<String>,
    #[serde(default)]
    pub include_sources: bool,
    #[serde(default)]
    pub include_ledger: bool,
    #[serde(default)]
    pub include_compiled_pdfs: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalReference {
    pub kind: String,
    pub id: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectEntry {
    pub kind: String,
    pub id: String,
    pub fingerprint: String,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobEntry {
    pub archive_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub role: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeManifest {
    pub format: String,
    pub format_version: u32,
    pub store_schema_version: u32,
    pub created_at: String,
    pub source_namespace: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_root: Option<String>,
    pub selection: ExchangeSelection,
    pub objects: Vec<ObjectEntry>,
    pub blobs: Vec<BlobEntry>,
    pub exclusions: Vec<String>,
    pub external_references: Vec<ExternalReference>,
    pub limitations: Vec<String>,
    pub reproducibility: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportExchangeRequest {
    pub workspace_id: String,
    pub path: String,
    pub selection: ExchangeSelection,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangePreview {
    pub objects: Vec<ObjectEntry>,
    pub blob_count: usize,
    pub blob_bytes: u64,
    pub exclusions: Vec<String>,
    pub external_references: Vec<ExternalReference>,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeReport {
    pub path: String,
    pub bytes: u64,
    pub package_hash: String,
    pub object_count: usize,
    pub blob_count: usize,
    pub exclusions: Vec<String>,
    pub external_references: Vec<ExternalReference>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeInspection {
    pub path: String,
    pub package_hash: String,
    pub workspace_name: String,
    pub source_namespace: String,
    pub created_at: String,
    pub workspace_root: Option<String>,
    pub counts: BTreeMap<String, usize>,
    pub blob_count: usize,
    pub blob_bytes: u64,
    pub exclusions: Vec<String>,
    pub external_references: usize,
    pub limitations: Vec<String>,
    pub own_export: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExchangeTarget {
    NewWorkspace { name: String, root: Option<String> },
    ExistingWorkspace { workspace_id: String },
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportExchangeRequest {
    pub path: String,
    pub target: ExchangeTarget,
    pub operation_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDecision {
    pub kind: String,
    pub id: String,
    pub outcome: String,
    pub new_id: Option<String>,
    pub title: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub target_workspace_id: Option<String>,
    pub new: usize,
    pub identical: usize,
    pub remapped: usize,
    pub conflicts: Vec<ObjectDecision>,
    pub decisions: Vec<ObjectDecision>,
    pub blob_count: usize,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub import_id: String,
    pub workspace_id: String,
    pub new: usize,
    pub identical: usize,
    pub remapped: usize,
    pub conflicts: usize,
    pub blobs: usize,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeConflict {
    pub id: String,
    pub import_id: String,
    pub object_kind: String,
    pub object_id: String,
    pub local: Value,
    pub base: Option<Value>,
    pub imported: Value,
    pub state: String,
    pub recorded_at: String,
}
