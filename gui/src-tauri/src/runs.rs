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

use crate::models::RunProducts;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
static RUN_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn title_words(value: &str) -> String {
    let words = value
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let mut text = words.join(" ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

fn agent_response_label(rel_path: &str) -> String {
    let stem = Path::new(rel_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(rel_path);
    let Some((producer_with_hash, attempt_and_status)) = stem.rsplit_once("--attempt-") else {
        return title_words(stem);
    };
    let producer = producer_with_hash
        .rsplit_once("--")
        .map(|(readable, _hash)| readable)
        .unwrap_or(producer_with_hash);
    let (attempt, remainder) = attempt_and_status
        .split_once('-')
        .unwrap_or((attempt_and_status, "response-captured"));
    let statuses = [
        ("rejected-envelope", "Rejected boundaries"),
        ("rejected-content", "Rejected content"),
        ("rejected-schema", "Rejected schema"),
        ("accepted", "Accepted"),
        ("ignored", "Not selected"),
        ("captured", "Captured before validation"),
    ];
    let (source, status) = statuses
        .iter()
        .find_map(|(suffix, label)| {
            remainder
                .strip_suffix(&format!("-{suffix}"))
                .map(|source| (source, *label))
        })
        .unwrap_or((remainder, "Captured response"));
    let attempt_display = attempt.trim_start_matches('0');
    let attempt_display = if attempt_display.is_empty() {
        "0"
    } else {
        attempt_display
    };
    format!(
        "{} · Attempt {} · {} · {}",
        title_words(producer),
        attempt_display,
        title_words(source),
        status,
    )
}

pub fn new_run_id(paper_hash: &str) -> String {
    let sequence = RUN_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}_{}-{sequence:016x}",
        paper_hash,
        chrono::Local::now().format("%Y%m%d-%H%M%S-%9f")
    )
}

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

pub fn input_identity(
    input_path: &str,
    interpretation: &str,
    content_hash: &str,
    inherited: Option<&InputIdentity>,
) -> InputIdentity {
    let selection = if input_path.trim().is_empty() {
        String::new()
    } else {
        let path = Path::new(input_path);
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .replace('\\', "/")
    };
    let selection_key = if selection.is_empty() {
        String::new()
    } else {
        format!("{}:{selection}", interpretation.trim())
    };
    let inherited_lineage = inherited
        .map(|identity| identity.lineage_id.trim())
        .filter(|lineage| !lineage.is_empty());
    let lineage_id = inherited_lineage.map(str::to_string).unwrap_or_else(|| {
        if selection_key.is_empty() {
            String::new()
        } else {
            let digest = Sha256::digest(selection_key.as_bytes());
            format!(
                "lineage-{}",
                digest[..8]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            )
        }
    });
    InputIdentity {
        selection_key,
        content_hash: content_hash.trim().to_string(),
        lineage_id,
    }
}

impl PageArtifactIndex {
    fn rel_path(&self, page: u32) -> Result<String, String> {
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

fn input_basename(input_path: &str) -> String {
    if input_path.trim().is_empty() {
        return "(no input)".to_string();
    }
    Path::new(input_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| input_path.to_string())
}

impl RunManifest {
    fn to_summary(&self) -> RunSummary {
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

pub fn runs_dir() -> Result<PathBuf, String> {
    let dir = crate::storage::data_root()?.join("runs");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create runs dir: {e}"))?;
    Ok(dir)
}

/// Run ids are `{hash}_{timestamp}` — same alphabet as validated paper hashes.
pub fn validate_run_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 96 {
        return Err("Invalid run id".into());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid run id".into());
    }
    Ok(())
}

/// Detect an artifact's kind from its extension, falling back to a NUL-byte
/// sniff for extensionless files. Deliberately coarse: the viewer only needs
/// to pick a renderer, not identify the language precisely.
pub fn detect_kind(rel_path: &str, head: &[u8]) -> &'static str {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" | "rmd" | "qmd" => "markdown",
        "json" => "json",
        "csv" | "tsv" => "csv",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => "image",
        "ado" | "mata" | "sty" | "cls" | "pyi" | "mjs" | "cjs" | "zsh" | "bash" | "ini" | "cfg"
        | "xml" | "jsonl" | "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "r" | "do" | "jl" | "c"
        | "cc" | "cpp" | "h" | "hpp" | "java" | "go" | "rb" | "sh" | "sql" | "tex" | "bib"
        | "toml" | "yaml" | "yml" | "html" | "css" | "m" | "f90" | "sas" | "stan" => "code",
        "txt" | "log" => "text",
        "pdf" => "pdf",
        "zip" | "gz" | "xlsx" | "docx" | "pptx" | "dta" | "rds" | "parquet" => "binary",
        _ => {
            if head.contains(&0) {
                "binary"
            } else {
                "text"
            }
        }
    }
}

fn short_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{:x}", digest)[..16].to_string()
}

fn inspect_file(path: &Path) -> Result<(Vec<u8>, u64, String), String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut head = Vec::with_capacity(512);
    let mut total = 0u64;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|e| format!("Failed to read file: {e}"))?;
        if count == 0 {
            break;
        }
        if head.len() < 512 {
            let take = count.min(512 - head.len());
            head.extend_from_slice(&chunk[..take]);
        }
        total += count as u64;
        hasher.update(&chunk[..count]);
    }
    Ok((
        head,
        total,
        format!("{:x}", hasher.finalize())[..16].to_string(),
    ))
}

fn read_at_most(path: &Path, limit: usize) -> Result<(Vec<u8>, bool), String> {
    use std::io::Read as _;
    let file =
        crate::safety::open_regular_file(path).map_err(|e| format!("Cannot open artifact: {e}"))?;
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024) + 1);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read artifact: {e}"))?;
    let truncated = bytes.len() > limit;
    if truncated {
        bytes.truncate(limit);
    }
    Ok((bytes, truncated))
}

/// Flush an already-written run file to disk, plus a best-effort sync of its
/// parent directory (matching `write_manifest`). Finalization deletes the step
/// checkpoints only after `report.json`/`report.md` are durable, so a power
/// loss in the writeback window cannot lose both copies of the step outputs.
pub(crate) fn sync_run_file(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("Failed to sync {}: {e}", path.display()))?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        let _ = fs::File::open(parent).and_then(|directory| directory.sync_all());
    }
    Ok(())
}

fn read_utf8_at_most(path: &Path, limit: usize, label: &str) -> Result<String, String> {
    let (bytes, oversized) = read_at_most(path, limit)?;
    if oversized {
        return Err(format!("{label} exceeds the {limit} byte safety limit"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{label} is not valid UTF-8: {e}"))
}

/// Accumulates artifacts for a run and writes the manifest at the end.
/// Required artifacts are published atomically. Callers must propagate write
/// failures rather than reporting a run as complete without durable results.
pub struct RunWriter {
    dir: PathBuf,
    run_id: String,
    artifacts: Vec<ArtifactEntry>,
    page_artifacts: Option<PageArtifactIndex>,
    created: String,
    meta: RunFinishMeta,
    finished: bool,
}

impl RunWriter {
    fn ensure_artifact_capacity(&self) -> Result<(), String> {
        if self.artifacts.len() >= MAX_MANIFEST_ARTIFACTS {
            return Err(format!(
                "Run has reached the {MAX_MANIFEST_ARTIFACTS}-artifact safety limit"
            ));
        }
        Ok(())
    }

    pub fn create(run_id: &str) -> Result<Self, String> {
        Self::create_in(&runs_dir()?, run_id)
    }

    pub fn create_unique(paper_hash: &str) -> Result<Self, String> {
        let base = runs_dir()?;
        for _ in 0..16 {
            let run_id = new_run_id(paper_hash);
            match Self::create_in(&base, &run_id) {
                Ok(writer) => return Ok(writer),
                Err(_) if base.join(&run_id).exists() => continue,
                Err(error) => return Err(error),
            }
        }
        Err("Could not allocate a unique run id".to_string())
    }

    fn create_in(base: &Path, run_id: &str) -> Result<Self, String> {
        validate_run_id(run_id)?;
        let dir = base.join(run_id);
        fs::create_dir(&dir).map_err(|e| format!("Failed to create run dir: {e}"))?;
        let writer = Self {
            dir,
            run_id: run_id.to_string(),
            artifacts: Vec::new(),
            page_artifacts: None,
            created: chrono::Local::now().to_rfc3339(),
            meta: RunFinishMeta {
                status: "running".to_string(),
                ..Default::default()
            },
            finished: false,
        };
        // A manifest is created before any artifacts. If a later stage fails,
        // the run remains visible to history and retention instead of becoming
        // an orphan directory.
        writer.persist_current()?;
        Ok(writer)
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Fill the pending manifest with information known at run start. The
    /// outcome remains `running` until `finish` or `Drop` finalizes it.
    pub fn set_pending_meta(&mut self, mut meta: RunFinishMeta) -> Result<(), String> {
        meta.status = "running".to_string();
        self.meta = meta;
        self.persist_current().map(|_| ())
    }

    pub fn record_extra_input(&mut self, key: &str, rel_path: &str) -> Result<(), String> {
        self.meta
            .extra_inputs
            .insert(key.to_string(), rel_path.to_string());
        self.persist_current().map(|_| ())
    }

    pub fn record_extra_input_source(&mut self, key: &str, path: &str) -> Result<(), String> {
        self.meta
            .extra_input_sources
            .insert(key.to_string(), path.to_string());
        self.persist_current().map(|_| ())
    }

    /// Write a text artifact at `rel_path` (forward slashes) and record it.
    pub fn add_text(
        &mut self,
        rel_path: &str,
        label: &str,
        group: &str,
        content: &str,
    ) -> Result<(), String> {
        self.add_bytes(rel_path, label, group, content.as_bytes())
    }

    fn add_bytes(
        &mut self,
        rel_path: &str,
        label: &str,
        group: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        self.ensure_artifact_capacity()?;
        let path = self.dir.join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {rel_path} parent: {e}"))?;
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| format!("Invalid artifact path: {rel_path}"))?;
            persistence::write_text_atomic(parent, name, bytes)
                .map_err(|e| format!("Failed to durably write {rel_path}: {e}"))?;
        } else {
            return Err(format!("Invalid artifact path: {rel_path}"));
        }
        self.artifacts.push(ArtifactEntry {
            rel_path: rel_path.to_string(),
            label: label.to_string(),
            kind: detect_kind(rel_path, &bytes[..bytes.len().min(512)]).to_string(),
            bytes: bytes.len() as u64,
            sha256: short_sha256(bytes),
            group: group.to_string(),
        });
        Ok(())
    }

    /// Preserve source files cited by canonical findings inside the immutable
    /// run. The finding keeps its human-facing source path and gains an
    /// `artifact_path` that the evidence viewer can open later, even if the
    /// selected source tree subsequently moves or changes.
    pub fn capture_source_evidence(
        &mut self,
        products: &mut RunProducts,
        source_root: &Path,
    ) -> Vec<String> {
        let Some(findings) = products.findings.as_mut() else {
            return Vec::new();
        };
        let has_uncaptured_source = findings.findings.iter().any(|finding| {
            finding.evidence.iter().any(|evidence| {
                evidence.artifact_path.is_empty() && !evidence.source_path.is_empty()
            })
        });
        if !has_uncaptured_source {
            return Vec::new();
        }
        if source_root.as_os_str().is_empty() {
            return vec!["Source evidence was cited but no source root was available".to_string()];
        }
        let root = match source_root.canonicalize() {
            Ok(root) if root.is_dir() => root,
            Ok(_) => return vec!["Source evidence root is not a directory".to_string()],
            Err(error) => return vec![format!("Could not resolve source evidence root: {error}")],
        };
        let mut warnings: Vec<String> = Vec::new();
        let mut captured: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();
        let mut captured_bytes = 0usize;

        for finding in &mut findings.findings {
            for evidence in &mut finding.evidence {
                if !evidence.artifact_path.is_empty() || evidence.source_path.is_empty() {
                    continue;
                }
                let source_path = evidence.source_path.clone();
                if let Some(existing) = captured.get(&source_path) {
                    if let Some(artifact_path) = existing {
                        evidence.artifact_path = artifact_path.clone();
                    }
                    continue;
                }
                if captured.len() >= MAX_SOURCE_EVIDENCE_FILES {
                    if !warnings
                        .iter()
                        .any(|warning| warning.contains("file-count limit"))
                    {
                        warnings.push(format!(
                            "Source evidence reached the {MAX_SOURCE_EVIDENCE_FILES}-file-count limit"
                        ));
                    }
                    captured.insert(source_path, None);
                    continue;
                }
                let relative = Path::new(&source_path);
                if source_path.starts_with('/')
                    || source_path.contains('\\')
                    || source_path.contains(':')
                    || !relative
                        .components()
                        .all(|component| matches!(component, Component::Normal(_)))
                {
                    warnings.push(format!(
                        "Rejected unsafe source evidence path '{source_path}'"
                    ));
                    captured.insert(source_path, None);
                    continue;
                }
                let resolved = match root.join(relative).canonicalize() {
                    Ok(path) if path.starts_with(&root) => path,
                    Ok(_) => {
                        warnings.push(format!(
                            "Rejected source evidence outside the selected root: '{source_path}'"
                        ));
                        captured.insert(source_path, None);
                        continue;
                    }
                    Err(error) => {
                        warnings.push(format!(
                            "Could not resolve source evidence '{source_path}': {error}"
                        ));
                        captured.insert(source_path, None);
                        continue;
                    }
                };
                let mut file = match crate::safety::open_regular_file(&resolved) {
                    Ok(file) => file,
                    Err(error) => {
                        warnings.push(format!(
                            "Could not open source evidence '{source_path}': {error}"
                        ));
                        captured.insert(source_path, None);
                        continue;
                    }
                };
                let mut bytes = Vec::new();
                use std::io::Read as _;
                if let Err(error) = file
                    .by_ref()
                    .take(MAX_SOURCE_EVIDENCE_FILE_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                {
                    warnings.push(format!(
                        "Could not read source evidence '{source_path}': {error}"
                    ));
                    captured.insert(source_path, None);
                    continue;
                }
                if bytes.len() > MAX_SOURCE_EVIDENCE_FILE_BYTES {
                    warnings.push(format!(
                        "Source evidence '{source_path}' exceeds the {} MB file limit",
                        MAX_SOURCE_EVIDENCE_FILE_BYTES / (1024 * 1024)
                    ));
                    captured.insert(source_path, None);
                    continue;
                }
                if captured_bytes.saturating_add(bytes.len()) > MAX_SOURCE_EVIDENCE_TOTAL_BYTES {
                    if !warnings
                        .iter()
                        .any(|warning| warning.contains("total byte limit"))
                    {
                        warnings.push(format!(
                            "Source evidence reached the {} MB total byte limit",
                            MAX_SOURCE_EVIDENCE_TOTAL_BYTES / (1024 * 1024)
                        ));
                    }
                    captured.insert(source_path, None);
                    continue;
                }
                let artifact_path = format!("context/source-evidence/{source_path}");
                match self.add_bytes(&artifact_path, &source_path, "files", &bytes) {
                    Ok(()) => {
                        captured_bytes += bytes.len();
                        evidence.artifact_path = artifact_path.clone();
                        captured.insert(source_path, Some(artifact_path));
                    }
                    Err(error) => {
                        warnings.push(format!(
                            "Could not save source evidence '{source_path}': {error}"
                        ));
                        captured.insert(source_path, None);
                    }
                }
            }
        }
        warnings
    }

    /// Record a file that already exists inside the run directory (e.g.
    /// page images rendered by pdftoppm) without rewriting it.
    pub fn register_existing(
        &mut self,
        rel_path: &str,
        label: &str,
        group: &str,
    ) -> Result<(), String> {
        self.ensure_artifact_capacity()?;
        let path = self.dir.join(rel_path);
        let (head, bytes, sha256) =
            inspect_file(&path).map_err(|e| format!("Failed to inspect {rel_path}: {e}"))?;
        self.artifacts.push(ArtifactEntry {
            rel_path: rel_path.to_string(),
            label: label.to_string(),
            kind: detect_kind(rel_path, &head).to_string(),
            bytes,
            sha256,
            group: group.to_string(),
        });
        Ok(())
    }

    /// Register files under `subdir` (relative to the run dir) that aren't
    /// already in the artifact index. Model-written supporting files land
    /// on disk without going through `add_text`; this picks them up at the
    /// end of the run. Symlinks are skipped, oversized files are skipped,
    /// and the walk is capped defensively. Returns how many were added.
    pub fn register_unlisted(&mut self, subdir: &str, group: &str) -> usize {
        const MAX_UNLISTED: usize = 500;
        const MAX_AGENT_RESPONSES: usize = 2_000;
        const MAX_UNLISTED_BYTES: u64 = 50_000_000;
        const MAX_TOTAL_UNLISTED_BYTES: u64 = 250_000_000;
        let known: std::collections::HashSet<String> =
            self.artifacts.iter().map(|a| a.rel_path.clone()).collect();
        let mut added = 0usize;
        let max_unlisted = if group == "agent_response" {
            MAX_AGENT_RESPONSES
        } else {
            MAX_UNLISTED
        };
        let mut total_bytes = 0u64;
        let mut stack = vec![self.dir.join(subdir)];
        let mut walk = crate::safety::WalkBudget::new("Run artifact discovery");
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else {
                continue;
            };
            for entry in entries.flatten() {
                if walk.entry().is_err() {
                    return added;
                }
                let path = entry.path();
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    if walk.directory().is_err() {
                        return added;
                    }
                    stack.push(path);
                    continue;
                }
                if !ft.is_file() {
                    // A CLI model can create FIFOs/sockets on Unix. Never open
                    // them during finalization; remove them from the run so a
                    // later artifact read cannot block either.
                    let _ = fs::remove_file(&path);
                    continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt as _;
                    if metadata.nlink() > 1 {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                }
                if metadata.len() > MAX_UNLISTED_BYTES
                    || total_bytes.saturating_add(metadata.len()) > MAX_TOTAL_UNLISTED_BYTES
                {
                    // These files were produced inside Pipeline's per-run
                    // model sandbox. Remove runaway output rather than leave
                    // an unindexed, unbounded disk leak behind.
                    let _ = fs::remove_file(&path);
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&self.dir) else {
                    continue;
                };
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                if known.contains(&rel_str) {
                    continue;
                }
                if added >= max_unlisted {
                    // Keep walking after the registration cap. Otherwise a
                    // model can place unlimited unindexed files after the
                    // first 500 and evade every byte quota.
                    let _ = fs::remove_file(&path);
                    continue;
                }
                let relative_label = rel_str
                    .strip_prefix(&format!("{subdir}/"))
                    .unwrap_or(&rel_str);
                let label = if group == "agent_response" {
                    agent_response_label(relative_label)
                } else {
                    relative_label.to_string()
                };
                if self.register_existing(&rel_str, &label, group).is_ok() {
                    added += 1;
                    total_bytes += metadata.len();
                }
            }
        }
        added
    }

    /// Replace homogeneous per-page manifest entries with one compact index.
    /// Files stay in place because providers, re-runs, and the on-demand page
    /// reader address them directly.
    pub fn compact_page_artifacts(&mut self) -> Result<usize, String> {
        let pages: Vec<(usize, u32, usize, String, u64)> = self
            .artifacts
            .iter()
            .enumerate()
            .filter(|(_, artifact)| artifact.group == "pages")
            .map(|(index, artifact)| {
                let path = Path::new(&artifact.rel_path);
                let parent = path
                    .parent()
                    .map(|value| value.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                if parent != "artifacts/pages" {
                    return Err(format!(
                        "Page artifact has an unexpected path: {}",
                        artifact.rel_path
                    ));
                }
                let extension = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_ascii_lowercase())
                    .ok_or_else(|| {
                        format!("Page artifact has no extension: {}", artifact.rel_path)
                    })?;
                if !matches!(extension.as_str(), "jpg" | "jpeg" | "png") {
                    return Err(format!("Unsupported page artifact format: {extension}"));
                }
                let digits = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .and_then(|value| value.strip_prefix("page-"))
                    .ok_or_else(|| {
                        format!(
                            "Page artifact has an unexpected name: {}",
                            artifact.rel_path
                        )
                    })?;
                if digits.is_empty() || !digits.chars().all(|value| value.is_ascii_digit()) {
                    return Err(format!(
                        "Page artifact has an invalid number: {}",
                        artifact.rel_path
                    ));
                }
                let page = digits
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid page number in {}", artifact.rel_path))?;
                Ok((index, page, digits.len(), extension, artifact.bytes))
            })
            .collect::<Result<_, String>>()?;
        if pages.is_empty() {
            return Ok(0);
        }
        let digit_width = pages[0].2;
        let extension = pages[0].3.clone();
        if digit_width == 0
            || digit_width > 8
            || pages
                .iter()
                .any(|(_, _, width, ext, _)| *width != digit_width || *ext != extension)
        {
            return Err("Page artifacts do not share one filename format".to_string());
        }
        let mut page_numbers: Vec<u32> = pages.iter().map(|(_, page, _, _, _)| *page).collect();
        page_numbers.sort_unstable();
        if page_numbers
            .iter()
            .copied()
            .ne(1..=page_numbers.len() as u32)
        {
            return Err(
                "Page artifacts are not a contiguous sequence starting at page 1".to_string(),
            );
        }
        let page_indices: std::collections::HashSet<usize> =
            pages.iter().map(|(index, _, _, _, _)| *index).collect();
        let total_bytes = pages.iter().fold(0u64, |total, (_, _, _, _, bytes)| {
            total.saturating_add(*bytes)
        });
        let count = pages.len();
        self.artifacts = self
            .artifacts
            .drain(..)
            .enumerate()
            .filter_map(|(index, artifact)| (!page_indices.contains(&index)).then_some(artifact))
            .collect();
        self.page_artifacts = Some(PageArtifactIndex {
            count: count as u32,
            digit_width: digit_width as u8,
            extension,
            total_bytes,
        });
        Ok(count)
    }

    /// Write manifest.json. Call once, last.
    pub fn finish(mut self, meta: RunFinishMeta) -> Result<RunManifest, String> {
        self.meta = meta;
        let manifest = self.current_manifest();
        write_manifest(&self.dir, &manifest)?;
        self.finished = true;
        Ok(manifest)
    }

    fn current_manifest(&self) -> RunManifest {
        // User metadata may be edited while finalization is racing with a UI
        // refresh. Preserve it instead of rebuilding those fields from empty
        // defaults on every lifecycle write.
        let (title, tags) = read_utf8_at_most(
            &self.dir.join("manifest.json"),
            MAX_MANIFEST_BYTES,
            "Run manifest",
        )
        .ok()
        .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok())
        .map(|manifest| (manifest.title, manifest.tags))
        .unwrap_or_default();
        RunManifest {
            artifact_schema_version: CURRENT_ARTIFACT_SCHEMA_VERSION,
            run_id: self.run_id.clone(),
            created: self.created.clone(),
            input_path: self.meta.input_path.clone(),
            input_mode: self.meta.input_mode.clone(),
            input_interpretation: self.meta.input_interpretation.clone(),
            input_identity: self.meta.input_identity.clone(),
            profile_id: self.meta.profile_id.clone(),
            profile_name: self.meta.profile_name.clone(),
            workflow_source: self.meta.workflow_source.clone(),
            workflow_fingerprint: self.meta.workflow_fingerprint.clone(),
            specialist_catalog_revision: self.meta.specialist_catalog_revision.clone(),
            provider: self.meta.provider.clone(),
            artifacts: self.artifacts.clone(),
            page_artifacts: self.page_artifacts.clone(),
            status: self.meta.status.clone(),
            duration_secs: self.meta.duration_secs,
            usage: self.meta.usage,
            step_count: self.meta.step_count,
            failed_steps: self.meta.failed_steps.clone(),
            title,
            tags,
            variables: self.meta.variables.clone(),
            extra_inputs: self.meta.extra_inputs.clone(),
            extra_input_sources: self.meta.extra_input_sources.clone(),
            parent_run_id: self.meta.parent_run_id.clone(),
        }
    }

    fn persist_current(&self) -> Result<RunManifest, String> {
        let manifest = self.current_manifest();
        write_manifest(&self.dir, &manifest)?;
        Ok(manifest)
    }
}

impl Drop for RunWriter {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.meta.status = if crate::commands::is_cancelled() {
            "cancelled".to_string()
        } else {
            "failed".to_string()
        };
        self.meta.usage = crate::pipeline::logging::run_usage();
        let _ = self.persist_current();
    }
}

mod artifacts;
mod history;
mod persistence;
mod retention;

pub use artifacts::{
    read_artifact, read_page_artifact, read_pdf_artifact_bytes, read_pdf_artifact_page,
};
pub(crate) use history::captured_document_rel_path;
pub use history::{
    delete_run, list_runs, list_trashed_runs, load_latest_report_for_input,
    permanently_delete_trashed_run, recover_resumable_runs, restore_trashed_run, update_run_meta,
    TrashedRun,
};
pub use persistence::load_manifest;
pub use retention::{
    disk_usage, preview_purge_runs_with_limits, purge_runs_with_expected_preview,
    purge_runs_with_limits, read_annotations, write_annotations,
};

use persistence::{
    recovery_failure, register_recovered_artifact, register_recovered_directory, write_manifest,
    write_text_atomic,
};

#[cfg(test)]
use artifacts::normalize_external_path;
#[cfg(test)]
use history::manifest_is_foreign_but_valid;
#[cfg(test)]
use history::DELETE_TOMBSTONE;
#[cfg(test)]
use history::{recover_orphan_manifest, recover_resumable_run_dir, run_has_resume_files};
#[cfg(test)]
use retention::{retention_plan_from_state, validate_annotation_content};

#[cfg(test)]
mod tests;
