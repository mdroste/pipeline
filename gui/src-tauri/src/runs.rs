//! Run directories: every pipeline run persists its outputs under
//! `~/.pipeline/runs/{run_id}/`:
//!
//! ```text
//! runs/{run_id}/
//! ├── manifest.json          # inputs, profile, artifact index
//! ├── report.md              # final rendered report
//! ├── context/               # extracted_text.md, orientation.json
//! └── artifacts/{step}/      # per-step outputs
//! ```
//!
//! The manifest is the frontend's source of truth: the artifact explorer
//! never walks the filesystem, and all file bytes flow through
//! `read_artifact` (one Tauri command with path validation and size caps)
//! rather than file:// URLs — that keeps the viewer identical across
//! WKWebView / WebView2 / WebKitGTK.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Text artifacts larger than this are truncated when read for display.
const MAX_TEXT_BYTES: usize = 1_000_000;
/// Images larger than this are not inlined (metadata only).
const MAX_IMAGE_BYTES: u64 = 10_000_000;
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;
const MAX_ANNOTATION_BYTES: usize = 1_000_000;
static RUN_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    /// Detected kind: markdown | code | json | csv | image | text | binary.
    pub kind: String,
    pub bytes: u64,
    /// First 16 hex chars of the SHA-256, matching the paper-hash style.
    pub sha256: String,
    /// Grouping hint for the explorer: report | context | step.
    pub group: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: String,
    pub created: String,
    pub input_path: String,
    pub input_mode: String,
    pub profile_id: String,
    pub profile_name: String,
    pub provider: String,
    pub artifacts: Vec<ArtifactEntry>,
    // ── Run-level metadata (all defaulted so pre-1.1 manifests still load) ──
    /// Outcome: "done" | "partial" (some steps failed) | "failed" | "cancelled".
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
    pub profile_id: String,
    pub profile_name: String,
    pub provider: String,
    pub status: String,
    pub duration_secs: u64,
    pub usage: crate::pipeline::logging::CallUsage,
    pub step_count: u32,
    pub failed_steps: Vec<String>,
    pub variables: std::collections::HashMap<String, String>,
    pub extra_inputs: std::collections::HashMap<String, String>,
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
    pub profile_id: String,
    pub profile_name: String,
    pub provider: String,
    pub status: String,
    pub duration_secs: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub step_count: u32,
    pub artifact_count: u32,
    pub failed_steps: Vec<String>,
    pub title: String,
    pub tags: Vec<String>,
}

/// Total number of runs on disk and the bytes they occupy.
#[derive(Debug, Clone, Serialize)]
pub struct RunsDiskUsage {
    pub count: u32,
    pub bytes: u64,
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
            profile_id: self.profile_id.clone(),
            profile_name: self.profile_name.clone(),
            provider: self.provider.clone(),
            status: if self.status.is_empty() {
                "done".to_string()
            } else {
                self.status.clone()
            },
            duration_secs: self.duration_secs,
            input_tokens: self.usage.input_tokens,
            output_tokens: self.usage.output_tokens,
            step_count: self.step_count,
            artifact_count: self.artifacts.len() as u32,
            failed_steps: self.failed_steps.clone(),
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

pub fn runs_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline").join("runs");
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
        "md" | "markdown" => "markdown",
        "json" => "json",
        "csv" | "tsv" => "csv",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => "image",
        "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "r" | "do" | "jl" | "c" | "cc" | "cpp"
        | "h" | "hpp" | "java" | "go" | "rb" | "sh" | "sql" | "tex" | "bib" | "toml" | "yaml"
        | "yml" | "html" | "css" | "m" | "f90" | "sas" | "stan" => "code",
        "txt" | "log" => "text",
        "pdf" | "zip" | "gz" | "xlsx" | "docx" | "pptx" | "dta" | "rds" | "parquet" => "binary",
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
    let file = fs::File::open(path).map_err(|e| format!("Failed to open file: {e}"))?;
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
    let file = fs::File::open(path).map_err(|e| format!("Cannot open artifact: {e}"))?;
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

fn read_utf8_at_most(path: &Path, limit: usize, label: &str) -> Result<String, String> {
    let (bytes, oversized) = read_at_most(path, limit)?;
    if oversized {
        return Err(format!("{label} exceeds the {limit} byte safety limit"));
    }
    String::from_utf8(bytes).map_err(|e| format!("{label} is not valid UTF-8: {e}"))
}

/// Accumulates artifacts for a run and writes the manifest at the end.
/// All writes are best-effort from the pipeline's perspective — callers log
/// failures but never fail the run because persistence failed.
pub struct RunWriter {
    dir: PathBuf,
    run_id: String,
    artifacts: Vec<ArtifactEntry>,
    created: String,
    meta: RunFinishMeta,
    finished: bool,
}

impl RunWriter {
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

    /// Write a text artifact at `rel_path` (forward slashes) and record it.
    pub fn add_text(
        &mut self,
        rel_path: &str,
        label: &str,
        group: &str,
        content: &str,
    ) -> Result<(), String> {
        let path = self.dir.join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {rel_path} parent: {e}"))?;
        }
        fs::write(&path, content).map_err(|e| format!("Failed to write {rel_path}: {e}"))?;
        let bytes = content.as_bytes();
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

    /// Record a file that already exists inside the run directory (e.g.
    /// page images rendered by pdftoppm) without rewriting it.
    pub fn register_existing(
        &mut self,
        rel_path: &str,
        label: &str,
        group: &str,
    ) -> Result<(), String> {
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
        const MAX_UNLISTED_BYTES: u64 = 50_000_000;
        const MAX_TOTAL_UNLISTED_BYTES: u64 = 250_000_000;
        let known: std::collections::HashSet<String> =
            self.artifacts.iter().map(|a| a.rel_path.clone()).collect();
        let mut added = 0usize;
        let mut total_bytes = 0u64;
        let mut stack = vec![self.dir.join(subdir)];
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else {
                continue;
            };
            for entry in entries.flatten() {
                if added >= MAX_UNLISTED {
                    return added;
                }
                let path = entry.path();
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
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
                let label = rel_str
                    .strip_prefix(&format!("{subdir}/"))
                    .unwrap_or(&rel_str)
                    .to_string();
                if self.register_existing(&rel_str, &label, group).is_ok() {
                    added += 1;
                    total_bytes += metadata.len();
                }
            }
        }
        added
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
        let (title, tags) = fs::read_to_string(self.dir.join("manifest.json"))
            .ok()
            .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok())
            .map(|manifest| (manifest.title, manifest.tags))
            .unwrap_or_default();
        RunManifest {
            run_id: self.run_id.clone(),
            created: self.created.clone(),
            input_path: self.meta.input_path.clone(),
            input_mode: self.meta.input_mode.clone(),
            profile_id: self.meta.profile_id.clone(),
            profile_name: self.meta.profile_name.clone(),
            provider: self.meta.provider.clone(),
            artifacts: self.artifacts.clone(),
            status: self.meta.status.clone(),
            duration_secs: self.meta.duration_secs,
            usage: self.meta.usage,
            step_count: self.meta.step_count,
            failed_steps: self.meta.failed_steps.clone(),
            title,
            tags,
            variables: self.meta.variables.clone(),
            extra_inputs: self.meta.extra_inputs.clone(),
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

/// Serialize a manifest to `{dir}/manifest.json`.
fn write_manifest(dir: &Path, manifest: &RunManifest) -> Result<(), String> {
    use std::io::Write as _;
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("Failed to serialize manifest: {e}"))?;
    let destination = dir.join("manifest.json");
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| format!("Failed to create manifest temp file: {e}"))?;
    temp.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write manifest temp file: {e}"))?;
    temp.flush()
        .map_err(|e| format!("Failed to flush manifest temp file: {e}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync manifest temp file: {e}"))?;
    temp.persist(&destination)
        .map_err(|e| format!("Failed to publish manifest: {}", e.error))?;
    #[cfg(unix)]
    {
        // Make the rename durable across a sudden power loss.
        fs::File::open(dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| format!("Failed to sync run directory: {e}"))?;
    }
    Ok(())
}

pub fn load_manifest(run_id: &str) -> Result<RunManifest, String> {
    validate_run_id(run_id)?;
    let path = runs_dir()?.join(run_id).join("manifest.json");
    let content = read_utf8_at_most(&path, MAX_MANIFEST_BYTES, "Run manifest")?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid manifest: {e}"))
}

/// List every run on disk as a summary row, newest first. Unreadable or
/// malformed manifests are skipped rather than failing the whole listing.
pub fn list_runs() -> Result<Vec<RunSummary>, String> {
    let dir = runs_dir()?;
    let mut summaries = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(summaries);
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        let manifest = read_utf8_at_most(&manifest_path, MAX_MANIFEST_BYTES, "Run manifest")
            .ok()
            .and_then(|content| serde_json::from_str::<RunManifest>(&content).ok())
            .or_else(|| recover_broken_manifest(&entry.path(), manifest_path.exists()));
        if let Some(manifest) = manifest {
            summaries.push(manifest.to_summary());
        }
    }
    // Sort by created timestamp (RFC3339 sorts lexically), newest first.
    summaries.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(summaries)
}

/// Locate the newest completed report in the same revision lineage. Stable
/// input path is the primary lineage key; content hash also finds identical
/// copies moved back to the same logical input.
pub fn load_latest_report_for_input(
    input_path: &str,
    paper_hash: &str,
) -> Result<Option<crate::models::PipelineReport>, String> {
    let normalized_input = Path::new(input_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(input_path));
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(runs_dir()?).map_err(|e| format!("Failed to list runs: {e}"))? {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let manifest = read_utf8_at_most(
            &entry.path().join("manifest.json"),
            MAX_MANIFEST_BYTES,
            "Run manifest",
        )
        .ok()
        .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok());
        let Some(manifest) = manifest else { continue };
        if manifest.status == "running"
            || manifest.status == "failed"
            || manifest.status == "cancelled"
        {
            continue;
        }
        let same_path = Path::new(&manifest.input_path)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(&manifest.input_path))
            == normalized_input;
        let same_hash = manifest.run_id.starts_with(&format!("{paper_hash}_"));
        if same_path || same_hash {
            candidates.push((manifest.created, entry.path().join("report.json")));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in candidates {
        let Ok(json) = read_utf8_at_most(&path, MAX_REPORT_BYTES, "Run report") else {
            continue;
        };
        if let Ok(report) = serde_json::from_str(&json) {
            return Ok(Some(report));
        }
    }
    Ok(None)
}

/// Make a pre-fix manifest-less run visible and eligible for normal retention.
/// The artifacts are left untouched; users can inspect the directory externally
/// or delete it from history.
fn recover_orphan_manifest(dir: &Path) -> Option<RunManifest> {
    let run_id = dir.file_name()?.to_str()?.to_string();
    validate_run_id(&run_id).ok()?;
    let created = dir
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(chrono::DateTime::<chrono::Local>::from)
        .unwrap_or_else(chrono::Local::now)
        .to_rfc3339();
    let manifest = RunManifest {
        run_id,
        created,
        input_path: String::new(),
        input_mode: String::new(),
        profile_id: String::new(),
        profile_name: String::new(),
        provider: String::new(),
        artifacts: Vec::new(),
        status: "failed".to_string(),
        duration_secs: 0,
        usage: Default::default(),
        step_count: 0,
        failed_steps: vec!["Run ended before manifest finalization".to_string()],
        title: String::new(),
        tags: Vec::new(),
        variables: Default::default(),
        extra_inputs: Default::default(),
        parent_run_id: None,
    };
    write_manifest(dir, &manifest).ok()?;
    Some(manifest)
}

fn recover_broken_manifest(dir: &Path, existed: bool) -> Option<RunManifest> {
    if existed {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let source = dir.join("manifest.json");
        let backup = dir.join(format!("manifest.corrupt-{stamp}.json"));
        // Preserve the bytes for support/recovery. If the rename loses a race
        // with a writer, leave the new manifest alone and try again next list.
        if fs::rename(source, backup).is_err() {
            return None;
        }
    }
    recover_orphan_manifest(dir)
}

/// Update a run's user-assigned title and tags in place. Tags are trimmed and
/// de-duplicated; empties are dropped.
pub fn update_run_meta(run_id: &str, title: &str, tags: &[String]) -> Result<(), String> {
    validate_run_id(run_id)?;
    let mut manifest = load_manifest(run_id)?;
    if manifest.status == "running" {
        return Err("A running job cannot be renamed or retagged".to_string());
    }
    manifest.title = title.trim().to_string();
    let mut seen = std::collections::HashSet::new();
    manifest.tags = tags
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && seen.insert(t.clone()))
        .collect();
    let dir = runs_dir()?.join(run_id);
    write_manifest(&dir, &manifest)
}

/// Delete a run directory and everything under it. The run id is validated and
/// the resolved path is confirmed to sit inside the runs directory before any
/// removal, so a crafted id can't escape the sandbox.
pub fn delete_run(run_id: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    let manifest = load_manifest(run_id)?;
    if manifest.status == "running" {
        return Err("A running job cannot be deleted".to_string());
    }
    let base = runs_dir()?
        .canonicalize()
        .map_err(|e| format!("Cannot resolve runs dir: {e}"))?;
    let dir = base.join(run_id);
    let canonical = dir
        .canonicalize()
        .map_err(|_| "Run not found".to_string())?;
    if !canonical.starts_with(&base) || canonical == base {
        return Err("Invalid run id".into());
    }
    fs::remove_dir_all(&canonical).map_err(|e| format!("Failed to delete run: {e}"))?;

    // Legacy history duplicated the full report (and often the extracted
    // paper text). Remove it once no remaining run references this paper hash
    // so deleting the final run really deletes the private report data.
    let paper_hash = run_id.split('_').next().unwrap_or_default();
    let another_run_exists = fs::read_dir(&base)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&format!("{paper_hash}_")))
        });
    if !another_run_exists && !paper_hash.is_empty() {
        if let Some(home) = dirs::home_dir() {
            let history = home.join(".pipeline").join("history");
            if let Ok(entries) = fs::read_dir(history) {
                for entry in entries.flatten() {
                    if entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.starts_with(&format!("{paper_hash}_")))
                    {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
    Ok(())
}

/// Count of runs on disk and total bytes they occupy (best-effort walk).
pub fn disk_usage() -> Result<RunsDiskUsage, String> {
    let dir = runs_dir()?;
    let mut count = 0u32;
    let mut bytes = 0u64;
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(RunsDiskUsage { count: 0, bytes: 0 });
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        count += 1;
        bytes += dir_size(&entry.path());
    }
    Ok(RunsDiskUsage { count, bytes })
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Read a run's annotations (per-issue accept/reject/note), or "{}" if none.
/// Annotations live beside the run in `annotations.json` and never touch the
/// report artifact.
pub fn read_annotations(run_id: &str) -> Result<String, String> {
    validate_run_id(run_id)?;
    load_manifest(run_id)?;
    let path = runs_dir()?.join(run_id).join("annotations.json");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("{}".to_string()),
        Err(error) => Err(format!("Cannot inspect annotations: {error}")),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("Annotations file cannot be a symlink".to_string())
        }
        Ok(_) => read_utf8_at_most(&path, MAX_ANNOTATION_BYTES, "Annotations file"),
    }
}

/// Write a run's annotations. `content` must be valid JSON and under 1 MB.
pub fn write_annotations(run_id: &str, content: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    validate_annotation_content(content)?;
    let dir = runs_dir()?.join(run_id);
    // A delayed frontend save must not recreate a run that was just deleted.
    load_manifest(run_id)?;
    let destination = dir.join("annotations.json");
    if fs::symlink_metadata(&destination)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("Annotations file cannot be a symlink".to_string());
    }
    let mut temp = tempfile::NamedTempFile::new_in(&dir)
        .map_err(|e| format!("Cannot create annotation temp file: {e}"))?;
    use std::io::Write as _;
    temp.write_all(content.as_bytes())
        .map_err(|e| format!("Cannot write annotations: {e}"))?;
    temp.flush()
        .map_err(|e| format!("Cannot flush annotations: {e}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Cannot sync annotations: {e}"))?;
    temp.persist(destination)
        .map_err(|e| format!("Cannot save annotations: {}", e.error))?;
    #[cfg(unix)]
    fs::File::open(&dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| format!("Cannot sync annotations directory: {e}"))?;
    Ok(())
}

fn validate_annotation_content(content: &str) -> Result<(), String> {
    if content.len() > MAX_ANNOTATION_BYTES {
        return Err("Annotations are too large".into());
    }
    let value = serde_json::from_str::<serde_json::Value>(content)
        .map_err(|e| format!("Annotations are not valid JSON: {e}"))?;
    if !value.is_object() {
        return Err("Annotations must be a JSON object".to_string());
    }
    Ok(())
}

/// Delete the oldest runs beyond `keep`, returning how many were removed.
/// `keep == 0` means unlimited (no purge). Best-effort: a delete failure on one
/// run doesn't stop the rest.
pub fn purge_old_runs(keep: usize) -> Result<usize, String> {
    if keep == 0 {
        return Ok(0);
    }
    let summaries = list_runs()?; // already newest-first
    let mut removed = 0usize;
    for summary in summaries.into_iter().skip(keep) {
        if delete_run(&summary.run_id).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Read an artifact for display. The single choke point for file bytes
/// reaching the webview: validates the path stays inside the run dir,
/// applies size caps, and never returns raw binary.
pub fn read_artifact(run_id: &str, rel_path: &str) -> Result<ArtifactContent, String> {
    validate_run_id(run_id)?;
    if rel_path.is_empty()
        || Path::new(rel_path).is_absolute()
        || rel_path.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("Invalid artifact path".into());
    }
    let run_dir = runs_dir()?
        .join(run_id)
        .canonicalize()
        .map_err(|_| "Run not found".to_string())?;
    let path = run_dir
        .join(rel_path)
        .canonicalize()
        .map_err(|_| "Artifact not found".to_string())?;
    if !path.starts_with(&run_dir) {
        return Err("Invalid artifact path".into());
    }

    let meta = fs::metadata(&path).map_err(|e| format!("Cannot stat artifact: {e}"))?;
    let size = meta.len();
    let abs_path = path.to_string_lossy().replace('\\', "/");

    // Sniff a small head for kind detection of extensionless files.
    let head = {
        use std::io::Read as _;
        let mut buf = vec![0u8; 512];
        let mut f = fs::File::open(&path).map_err(|e| format!("Cannot open artifact: {e}"))?;
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("Cannot read artifact: {e}"))?;
        buf.truncate(n);
        buf
    };
    let kind = detect_kind(rel_path, &head);

    match kind {
        "image" => {
            if size > MAX_IMAGE_BYTES {
                return Ok(ArtifactContent {
                    kind: kind.into(),
                    bytes: size,
                    text: None,
                    base64: None,
                    truncated: false,
                    abs_path,
                });
            }
            let (bytes, grew_too_large) = read_at_most(&path, MAX_IMAGE_BYTES as usize)?;
            if grew_too_large {
                return Ok(ArtifactContent {
                    kind: kind.into(),
                    bytes: size,
                    text: None,
                    base64: None,
                    truncated: false,
                    abs_path,
                });
            }
            use base64::Engine as _;
            Ok(ArtifactContent {
                kind: kind.into(),
                bytes: size,
                text: None,
                base64: Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
                truncated: false,
                abs_path,
            })
        }
        "binary" => Ok(ArtifactContent {
            kind: kind.into(),
            bytes: size,
            text: None,
            base64: None,
            truncated: false,
            abs_path,
        }),
        _ => {
            let (bytes, truncated) = read_at_most(&path, MAX_TEXT_BYTES)?;
            Ok(ArtifactContent {
                kind: kind.into(),
                bytes: size,
                text: Some(String::from_utf8_lossy(&bytes).into_owned()),
                base64: None,
                truncated,
                abs_path,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_kind_by_extension_and_sniff() {
        assert_eq!(detect_kind("report.md", b""), "markdown");
        assert_eq!(detect_kind("steps/01_technical.md", b""), "markdown");
        assert_eq!(detect_kind("orientation.json", b""), "json");
        assert_eq!(detect_kind("analysis.py", b""), "code");
        assert_eq!(detect_kind("data.csv", b""), "csv");
        assert_eq!(detect_kind("fig.png", b""), "image");
        assert_eq!(detect_kind("paper.pdf", b""), "binary");
        assert_eq!(detect_kind("README", b"plain text"), "text");
        assert_eq!(detect_kind("blob", b"\x00\x01\x02"), "binary");
    }

    #[test]
    fn run_id_validation() {
        assert!(validate_run_id("abc123_20260702-120000").is_ok());
        assert!(validate_run_id("").is_err());
        assert!(validate_run_id("../escape").is_err());
        assert!(validate_run_id("a/b").is_err());
    }

    #[test]
    fn read_artifact_rejects_traversal() {
        assert!(read_artifact("some-run", "../other/file.md").is_err());
        assert!(read_artifact("some-run", "/etc/passwd").is_err());
        assert!(read_artifact("some-run", "a/../../b").is_err());
    }

    #[test]
    fn bounded_utf8_reader_stops_at_the_limit() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        fs::write(temp.path(), b"123456789").unwrap();
        assert_eq!(
            read_utf8_at_most(temp.path(), 9, "test").unwrap(),
            "123456789"
        );
        assert!(read_utf8_at_most(temp.path(), 8, "test").is_err());
    }

    #[test]
    fn annotations_require_a_bounded_json_object() {
        assert!(validate_annotation_content(r#"{"a":{"status":"done"}}"#).is_ok());
        assert!(validate_annotation_content("[]").is_err());
        assert!(validate_annotation_content("not-json").is_err());
        assert!(validate_annotation_content(&"x".repeat(MAX_ANNOTATION_BYTES + 1)).is_err());
    }

    #[test]
    fn old_manifest_without_metadata_loads() {
        // A pre-1.1 manifest has none of the run-level metadata fields.
        let json = r#"{
            "run_id": "abc_20260101-000000",
            "created": "2026-01-01T00:00:00+00:00",
            "input_path": "/papers/main.pdf",
            "input_mode": "document",
            "profile_id": "deep-review",
            "profile_name": "Deep Review",
            "provider": "claude",
            "artifacts": []
        }"#;
        let m: RunManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.status, "");
        assert_eq!(m.duration_secs, 0);
        assert_eq!(m.usage.input_tokens, 0);
        assert!(m.tags.is_empty());
        // Summary fills a sensible default status and derives the input name.
        let s = m.to_summary();
        assert_eq!(s.status, "done");
        assert_eq!(s.input_name, "main.pdf");
    }

    #[test]
    fn summary_carries_metrics_and_basename() {
        let m = RunManifest {
            run_id: "r1".into(),
            created: "2026-07-07T10:00:00+00:00".into(),
            input_path: "/home/u/paper.tex".into(),
            input_mode: "document".into(),
            profile_id: "deep-review".into(),
            profile_name: "Deep Review".into(),
            provider: "claude".into(),
            artifacts: vec![],
            status: "partial".into(),
            duration_secs: 125,
            usage: crate::pipeline::logging::CallUsage {
                input_tokens: 1000,
                output_tokens: 200,
            },
            step_count: 6,
            failed_steps: vec!["Empirical".into()],
            title: "My run".into(),
            tags: vec!["urgent".into()],
            variables: std::collections::HashMap::new(),
            extra_inputs: std::collections::HashMap::new(),
            parent_run_id: None,
        };
        let s = m.to_summary();
        assert_eq!(s.input_name, "paper.tex");
        assert_eq!(s.status, "partial");
        assert_eq!(s.input_tokens, 1000);
        assert_eq!(s.step_count, 6);
        assert_eq!(s.failed_steps, vec!["Empirical".to_string()]);
        assert_eq!(s.title, "My run");
    }

    #[test]
    fn input_basename_handles_empty_and_paths() {
        assert_eq!(input_basename(""), "(no input)");
        assert_eq!(input_basename("   "), "(no input)");
        assert_eq!(input_basename("/a/b/c.pdf"), "c.pdf");
        assert_eq!(input_basename("relative.tex"), "relative.tex");
    }

    #[test]
    fn unfinished_writer_keeps_a_failed_manifest_and_artifact_index() {
        let temp = tempfile::tempdir().unwrap();
        {
            let mut writer = RunWriter::create_in(temp.path(), "run-1").unwrap();
            writer
                .add_text("context/input.md", "Input", "context", "hello")
                .unwrap();
            writer
                .record_extra_input("letter", "context/input.md")
                .unwrap();
        }

        let content = fs::read_to_string(temp.path().join("run-1/manifest.json")).unwrap();
        let manifest: RunManifest = serde_json::from_str(&content).unwrap();
        assert_eq!(manifest.status, "failed");
        assert_eq!(manifest.artifacts.len(), 1);
        assert_eq!(
            manifest.extra_inputs.get("letter").map(String::as_str),
            Some("context/input.md")
        );
    }

    #[test]
    fn finished_writer_is_not_overwritten_by_drop() {
        let temp = tempfile::tempdir().unwrap();
        let writer = RunWriter::create_in(temp.path(), "run-2").unwrap();
        writer
            .finish(RunFinishMeta {
                status: "done".into(),
                ..Default::default()
            })
            .unwrap();

        let content = fs::read_to_string(temp.path().join("run-2/manifest.json")).unwrap();
        let manifest: RunManifest = serde_json::from_str(&content).unwrap();
        assert_eq!(manifest.status, "done");
    }

    #[test]
    fn manifestless_directory_is_recovered_as_failed() {
        let temp = tempfile::tempdir().unwrap();
        let orphan = temp.path().join("old-run");
        fs::create_dir(&orphan).unwrap();
        fs::write(orphan.join("partial.log"), "unfinished").unwrap();

        let manifest = recover_orphan_manifest(&orphan).unwrap();
        assert_eq!(manifest.status, "failed");
        assert!(orphan.join("manifest.json").is_file());
    }

    #[test]
    fn capped_artifact_reader_never_loads_past_limit() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write as _;
        file.write_all(&vec![b'x'; 4096]).unwrap();
        let (bytes, truncated) = read_at_most(file.path(), 128).unwrap();
        assert_eq!(bytes.len(), 128);
        assert!(truncated);
    }

    #[test]
    fn run_ids_are_unique_and_directories_are_exclusive() {
        assert_ne!(new_run_id("abc"), new_run_id("abc"));
        let temp = tempfile::tempdir().unwrap();
        let writer = RunWriter::create_in(temp.path(), "same-id").unwrap();
        assert!(RunWriter::create_in(temp.path(), "same-id").is_err());
        drop(writer);
    }
}
