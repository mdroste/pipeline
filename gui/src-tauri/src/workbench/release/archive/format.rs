//! Portable research archive format, limits, and command DTOs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(super) const ARCHIVE_FORMAT_VERSION: u32 = 2;
pub(super) const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(super) const MAX_ARCHIVE_ENTRIES: usize = 20_000;
pub(super) const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;
pub(super) const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BlobManifest {
    pub(super) archive_path: String,
    pub(super) original_reference: String,
    pub(super) size_bytes: u64,
    pub(super) sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArchiveManifest {
    pub(super) format: String,
    pub(super) format_version: u32,
    pub(super) store_schema_version: u32,
    pub(super) created_at: String,
    pub(super) original_blob_root: String,
    #[serde(default)]
    pub(super) workspace_roots: Vec<String>,
    pub(super) blobs: Vec<BlobManifest>,
    #[serde(default)]
    pub(super) conversation_files: Vec<BlobManifest>,
    pub(super) portability_note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportArchiveRequest {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportArchiveRequest {
    pub path: String,
    #[serde(default)]
    pub root_mappings: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveReport {
    pub path: String,
    pub workspace_count: usize,
    pub session_count: usize,
    pub blob_count: usize,
    pub bytes: u64,
    pub native_bindings_retired: usize,
    pub portability_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInspection {
    pub path: String,
    pub format_version: u32,
    pub store_schema_version: u32,
    pub workspace_roots: Vec<String>,
    pub portability_note: String,
}
