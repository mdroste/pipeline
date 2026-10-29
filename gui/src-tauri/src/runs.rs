//! Run directories: every pipeline run persists its outputs under
//! `~/.pipeline/runs/{run_id}/`:
//!
//! ```text
//! runs/{run_id}/
//! ├── manifest.json          # inputs, profile, artifact index
//! ├── report.md              # final rendered report
//! ├── context/               # document.md, orientation.json
//! └── artifacts/{step}/      # per-step outputs
//! ```
//!
//! The manifest is the frontend's source of truth: the artifact explorer
//! never walks the filesystem. Source bytes flow through path-validated,
//! size-capped Tauri commands; PDF artifacts cross the boundary only as
//! derived page images. No viewer uses file:// URLs, keeping behavior
//! consistent across WKWebView / WebView2 / WebKitGTK.

use std::fs;
use std::path::{Path, PathBuf};

/// Text artifacts larger than this are truncated when read for display.
const MAX_TEXT_BYTES: usize = 1_000_000;
const MAX_DOCUMENT_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
/// Images larger than this are not inlined (metadata only).
const MAX_IMAGE_BYTES: u64 = 10_000_000;
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_MANIFEST_ARTIFACTS: usize = 5_000;

const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;
const MAX_ANNOTATION_BYTES: usize = 1_000_000;
const MAX_SOURCE_EVIDENCE_FILES: usize = 200;
const MAX_SOURCE_EVIDENCE_FILE_BYTES: usize = 5 * 1024 * 1024;
const MAX_SOURCE_EVIDENCE_TOTAL_BYTES: usize = 50 * 1024 * 1024;
const CURRENT_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub(crate) const DOCUMENT_TEXT_PATH: &str = "context/document.md";
const LEGACY_EXTRACTED_TEXT_PATH: &str = "context/extracted_text.md";

mod artifacts;
mod history;
mod identity;
mod inspection;
mod persistence;
mod records;
mod recovery;
mod retention;
mod trash;
mod unlisted;
mod writer;

pub use artifacts::{
    read_artifact, read_page_artifact, read_pdf_artifact_bytes, read_pdf_artifact_page,
};
pub use history::{list_runs, load_latest_report_for_input, update_run_meta};
pub use identity::{
    input_identity, new_run_id, open_run_lock, run_lock_path, runs_dir, validate_run_id,
};
pub use inspection::detect_kind;
pub(crate) use inspection::sync_run_file;
pub use persistence::load_manifest;
pub use records::{
    ArtifactContent, ArtifactEntry, InputIdentity, PageArtifactIndex, PdfArtifactPage,
    PrefetchedPdfArtifactPage, RunFinishMeta, RunManifest, RunPurgePreview, RunSummary,
    RunsDiskUsage,
};
pub(crate) use recovery::captured_document_rel_path;
pub use recovery::recover_resumable_runs;
pub use retention::{
    disk_usage, preview_purge_runs_with_limits, purge_runs_with_expected_preview,
    purge_runs_with_limits, read_annotations, write_annotations,
};
pub use trash::{
    delete_run, list_trashed_runs, permanently_delete_trashed_run, restore_trashed_run, TrashedRun,
};
pub use writer::RunWriter;

#[cfg(test)]
use artifacts::normalize_external_path;
use inspection::{
    agent_response_label, inspect_file, read_at_most, read_utf8_at_most, short_sha256,
};
use persistence::{
    recovery_failure, register_recovered_artifact, register_recovered_directory, write_manifest,
    write_text_atomic,
};
#[cfg(test)]
use records::input_basename;
#[cfg(test)]
use recovery::{
    manifest_is_foreign_but_valid, recover_orphan_manifest, recover_resumable_run_dir,
    recover_resumable_runs_at, run_has_resume_files, DELETE_TOMBSTONE,
};
#[cfg(test)]
use retention::{retention_plan_from_state, validate_annotation_content};
#[cfg(test)]
use trash::restore_trashed_run_at;
#[cfg(test)]
mod tests;
