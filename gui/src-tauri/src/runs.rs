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
const MAX_DOCUMENT_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
/// Images larger than this are not inlined (metadata only).
const MAX_IMAGE_BYTES: u64 = 10_000_000;
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_MANIFEST_ARTIFACTS: usize = 5_000;
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
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub step_count: u32,
    pub artifact_count: u32,
    pub failed_steps: Vec<String>,
    /// The run has a structured report plus cached extraction and can continue
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
            cached_input_tokens: self.usage.cached_input_tokens,
            cache_write_input_tokens: self.usage.cache_write_input_tokens,
            step_count: self.step_count,
            artifact_count: self.artifacts.len() as u32,
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
        self.ensure_artifact_capacity()?;
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
        const MAX_UNLISTED_BYTES: u64 = 50_000_000;
        const MAX_TOTAL_UNLISTED_BYTES: u64 = 250_000_000;
        let known: std::collections::HashSet<String> =
            self.artifacts.iter().map(|a| a.rel_path.clone()).collect();
        let mut added = 0usize;
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
                if added >= MAX_UNLISTED {
                    // Keep walking after the registration cap. Otherwise a
                    // model can place unlimited unindexed files after the
                    // first 500 and evade every byte quota.
                    let _ = fs::remove_file(&path);
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
    if json.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "Run manifest would be {} bytes; the safety limit is {MAX_MANIFEST_BYTES}",
            json.len()
        ));
    }
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

fn write_text_atomic(dir: &Path, name: &str, content: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let destination = dir.join(name);
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|error| format!("Failed to create recovery temp file: {error}"))?;
    temp.write_all(content)
        .map_err(|error| format!("Failed to write recovery temp file: {error}"))?;
    temp.flush()
        .map_err(|error| format!("Failed to flush recovery temp file: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("Failed to sync recovery temp file: {error}"))?;
    temp.persist(destination)
        .map_err(|error| format!("Failed to publish recovered file: {}", error.error))?;
    Ok(())
}

fn register_recovered_artifact(
    manifest: &mut RunManifest,
    dir: &Path,
    rel_path: &str,
    label: &str,
    group: &str,
) {
    if manifest.artifacts.len() >= MAX_MANIFEST_ARTIFACTS
        || manifest
            .artifacts
            .iter()
            .any(|artifact| artifact.rel_path == rel_path)
    {
        return;
    }
    let path = dir.join(rel_path);
    let Ok((head, bytes, sha256)) = inspect_file(&path) else {
        return;
    };
    manifest.artifacts.push(ArtifactEntry {
        rel_path: rel_path.to_string(),
        label: label.to_string(),
        kind: detect_kind(rel_path, &head).to_string(),
        bytes,
        sha256,
        group: group.to_string(),
    });
}

fn register_recovered_directory(
    manifest: &mut RunManifest,
    dir: &Path,
    rel_dir: &str,
    group: &str,
) {
    let Ok(entries) = fs::read_dir(dir.join(rel_dir)) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            (file_type.is_file() && !file_type.is_symlink()).then_some(entry.path())
        })
        .collect();
    paths.sort();
    for path in paths.into_iter().take(500) {
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let relative = format!("{rel_dir}/{name}");
        let label = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(name);
        register_recovered_artifact(manifest, dir, &relative, label, group);
    }
}

fn recovery_failure(manifest: &RunManifest) -> Vec<crate::models::StepFailure> {
    if manifest.status == "done" {
        return Vec::new();
    }
    if manifest.status == "partial" && !manifest.failed_steps.is_empty() {
        return manifest
            .failed_steps
            .iter()
            .enumerate()
            .map(|(index, label)| crate::models::StepFailure {
                step_id: format!("__incomplete_{index}__"),
                step_label: label.clone(),
                error: "Step did not complete before the run stopped".to_string(),
            })
            .collect();
    }
    let (step_id, step_label, error) = match manifest.status.as_str() {
        "cancelled" => (
            "__run_cancelled__",
            "Run cancelled",
            "Pipeline was cancelled before final report finalization",
        ),
        "failed" => (
            "__run_failed__",
            "Run failed",
            "Pipeline failed before final report finalization",
        ),
        "partial" => (
            "__run_partial__",
            "Run incomplete",
            "Pipeline stopped before its structured report was saved",
        ),
        _ => (
            "__run_interrupted__",
            "Run interrupted",
            "Pipeline exited before final report finalization",
        ),
    };
    vec![crate::models::StepFailure {
        step_id: step_id.to_string(),
        step_label: step_label.to_string(),
        error: error.to_string(),
    }]
}

fn resumable_status(status: &str) -> bool {
    matches!(status, "partial" | "failed" | "cancelled" | "interrupted")
}

fn run_has_resume_files(dir: &Path, manifest: &RunManifest) -> bool {
    if !resumable_status(&manifest.status)
        || crate::safety::open_regular_file(&dir.join("context/extracted_text.md")).is_err()
    {
        return false;
    }
    read_utf8_at_most(&dir.join("report.json"), MAX_REPORT_BYTES, "Run report")
        .ok()
        .and_then(|json| serde_json::from_str::<crate::models::PipelineReport>(&json).ok())
        .is_some()
}

/// Turn the durable pieces of an unfinished run into the same structured
/// report consumed by the normal resume path. Returns true only when recovery
/// wrote a report; completed/unsupported runs are left untouched.
fn recover_resumable_run_dir(dir: &Path, manifest: &mut RunManifest) -> Result<bool, String> {
    if !matches!(
        manifest.status.as_str(),
        "running" | "partial" | "failed" | "cancelled" | "interrupted" | "done"
    ) {
        return Ok(false);
    }
    let report_is_valid =
        read_utf8_at_most(&dir.join("report.json"), MAX_REPORT_BYTES, "Run report")
            .ok()
            .and_then(|json| serde_json::from_str::<crate::models::PipelineReport>(&json).ok())
            .is_some();
    if report_is_valid {
        return Ok(false);
    }
    // A re-run always starts from the captured extraction. If it was never
    // written, this job stopped before there was a safe restart point.
    crate::safety::open_regular_file(&dir.join("context/extracted_text.md"))
        .map_err(|_| "Incomplete run has no captured extraction to resume from".to_string())?;

    let checkpoint_dir = dir.join("artifacts").join("checkpoints");
    let mut checkpoints = Vec::new();
    if let Ok(entries) = fs::read_dir(&checkpoint_dir) {
        let mut walk = crate::safety::WalkBudget::new("Incomplete-run checkpoint recovery");
        for entry in entries.flatten() {
            walk.entry()?;
            let path = entry.path();
            let is_regular = entry
                .file_type()
                .map(|kind| kind.is_file() && !kind.is_symlink())
                .unwrap_or(false);
            if !is_regular || path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;
                if entry
                    .metadata()
                    .map(|metadata| metadata.nlink() > 1)
                    .unwrap_or(true)
                {
                    continue;
                }
            }
            checkpoints.push(path);
        }
    }
    checkpoints.sort();

    let mut outputs = Vec::new();
    let mut checkpoint_failures = Vec::new();
    for path in &checkpoints {
        let Ok(json) = read_utf8_at_most(path, MAX_REPORT_BYTES, "Step checkpoint") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
            continue;
        };
        if let Some(failure) = value
            .get("failure")
            .cloned()
            .and_then(|failure| serde_json::from_value(failure).ok())
        {
            checkpoint_failures.push(failure);
        } else if let Ok(output) = serde_json::from_value::<crate::models::StepOutput>(value) {
            outputs.push(output);
        }
    }

    let orientation = read_utf8_at_most(
        &dir.join("context").join("orientation.json"),
        MAX_REPORT_BYTES,
        "Orientation map",
    )
    .ok()
    .and_then(|json| serde_json::from_str(&json).ok())
    .unwrap_or(serde_json::Value::Null);
    let failed_steps = if checkpoint_failures.is_empty() {
        recovery_failure(manifest)
    } else {
        checkpoint_failures
    };
    let report = crate::models::PipelineReport {
        orientation,
        step_outputs: outputs,
        failed_steps,
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::DateTime::parse_from_rfc3339(&manifest.created)
            .map(|date| date.date_naive())
            .unwrap_or_else(|_| chrono::Local::now().date_naive()),
        paper_hash: manifest
            .run_id
            .split('_')
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    let report_json = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("Failed to serialize recovered report: {error}"))?;
    write_text_atomic(dir, "report.json", &report_json)?;
    let settings = crate::settings::load();
    let markdown = crate::output::render_markdown(
        &report,
        None,
        std::time::Duration::from_secs(manifest.duration_secs),
        &settings,
    );
    write_text_atomic(dir, "report.md", markdown.as_bytes())?;

    register_recovered_artifact(manifest, dir, "report.md", "Recovered report", "report");
    register_recovered_artifact(
        manifest,
        dir,
        "report.json",
        "Recovered report data",
        "context",
    );
    register_recovered_artifact(
        manifest,
        dir,
        "context/document_bundle.json",
        "Document bundle",
        "document",
    );
    register_recovered_artifact(
        manifest,
        dir,
        "context/document.md",
        "Readable document",
        "document",
    );
    register_recovered_artifact(
        manifest,
        dir,
        "context/blocks.jsonl",
        "Document blocks",
        "document",
    );
    register_recovered_directory(manifest, dir, "artifacts/pages", "pages");
    register_recovered_directory(manifest, dir, "artifacts/figures", "figures");
    register_recovered_directory(manifest, dir, "artifacts/document/figures", "figures");
    register_recovered_artifact(
        manifest,
        dir,
        "context/extracted_text.md",
        "Extracted text",
        "context",
    );
    register_recovered_artifact(
        manifest,
        dir,
        "context/orientation.json",
        "Orientation map",
        "context",
    );
    for path in checkpoints {
        let Ok(relative) = path.strip_prefix(dir) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        let label = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Step checkpoint");
        register_recovered_artifact(manifest, dir, &relative, label, "checkpoint");
    }

    if manifest.status == "running" {
        manifest.status = "interrupted".to_string();
    }
    manifest.step_count = report.step_outputs.len() as u32;
    for failure in &report.failed_steps {
        if !manifest
            .failed_steps
            .iter()
            .any(|label| label == &failure.step_label)
        {
            manifest.failed_steps.push(failure.step_label.clone());
        }
    }
    write_manifest(dir, manifest)?;
    Ok(true)
}

/// Recover runs stopped by an abort, cancellation, provider failure, or a
/// report-persistence failure. The global run lock is acquired first, so a
/// second Pipeline process can never rewrite a genuinely active run.
/// Completed step checkpoints become a partial `report.json`, allowing the
/// normal re-run path to reuse them.
pub fn recover_resumable_runs() -> Result<usize, String> {
    use fs2::FileExt as _;
    let root = runs_dir()?;
    let pipeline_dir = root
        .parent()
        .ok_or("Cannot resolve Pipeline data directory")?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(pipeline_dir.join("run.lock"))
        .map_err(|error| format!("Failed to open Pipeline recovery lock: {error}"))?;
    if lock.try_lock_exclusive().is_err() {
        return Ok(0);
    }

    let mut recovered = 0usize;
    let mut walk = crate::safety::WalkBudget::new("Incomplete-run recovery");
    for entry in fs::read_dir(&root)
        .map_err(|error| format!("Failed to list runs for recovery: {error}"))?
        .flatten()
    {
        walk.entry()?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        let Ok(json) = read_utf8_at_most(&manifest_path, MAX_MANIFEST_BYTES, "Run manifest") else {
            continue;
        };
        let Ok(mut manifest) = serde_json::from_str::<RunManifest>(&json) else {
            continue;
        };
        if recover_resumable_run_dir(&entry.path(), &mut manifest).unwrap_or(false) {
            recovered += 1;
        }
    }
    let _ = fs2::FileExt::unlock(&lock);
    Ok(recovered)
}

/// List every run on disk as a summary row, newest first. Unreadable or
/// malformed manifests are skipped rather than failing the whole listing.
pub fn list_runs() -> Result<Vec<RunSummary>, String> {
    let dir = runs_dir()?;
    let mut summaries = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(summaries);
    };
    let mut walk = crate::safety::WalkBudget::new("Run history listing");
    for entry in entries.flatten() {
        walk.entry()?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        let manifest_too_large = fs::symlink_metadata(&manifest_path)
            .map(|metadata| metadata.len() > MAX_MANIFEST_BYTES as u64)
            .unwrap_or(false);
        let manifest = if manifest_too_large {
            // Never rename and replace an oversized manifest: doing so can
            // destroy a valid run index merely because an older writer
            // exceeded the newer reader's limit.
            None
        } else {
            read_utf8_at_most(&manifest_path, MAX_MANIFEST_BYTES, "Run manifest")
                .ok()
                .and_then(|content| serde_json::from_str::<RunManifest>(&content).ok())
                .or_else(|| recover_broken_manifest(&entry.path(), manifest_path.exists()))
        };
        if let Some(manifest) = manifest {
            let mut summary = manifest.to_summary();
            summary.resumable = run_has_resume_files(&entry.path(), &manifest);
            summaries.push(summary);
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
    let mut walk = crate::safety::WalkBudget::new_cancellable("Prior report discovery");
    for entry in fs::read_dir(runs_dir()?).map_err(|e| format!("Failed to list runs: {e}"))? {
        walk.entry()?;
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
            || manifest.status == "interrupted"
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
    let title = title.trim();
    if title.len() > 500 {
        return Err("Run title cannot exceed 500 bytes".to_string());
    }
    if tags.len() > 50 || tags.iter().any(|tag| tag.trim().len() > 100) {
        return Err("Runs support at most 50 tags of 100 bytes each".to_string());
    }
    manifest.title = title.to_string();
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
    let mut walk = crate::safety::WalkBudget::new("Run storage scan");
    for entry in entries.flatten() {
        walk.entry()?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        count += 1;
        walk.directory()?;
        bytes = bytes.saturating_add(dir_size(&entry.path(), &mut walk)?);
    }
    Ok(RunsDiskUsage { count, bytes })
}

fn dir_size(path: &Path, walk: &mut crate::safety::WalkBudget) -> Result<u64, String> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            walk.entry()?;
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                walk.directory()?;
                stack.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Ok(total)
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

/// Delete oldest completed runs until both count and byte limits hold. A zero
/// limit disables that dimension. Pending/running runs are never candidates.
pub fn purge_runs_with_limits(keep: usize, max_bytes: u64) -> Result<usize, String> {
    if keep == 0 && max_bytes == 0 {
        return Ok(0);
    }
    let summaries = list_runs()?; // already newest-first
    let root = runs_dir()?;
    let mut walk = crate::safety::WalkBudget::new("Run retention scan");
    let mut sized: Vec<(RunSummary, u64)> = Vec::with_capacity(summaries.len());
    for summary in summaries {
        walk.directory()?;
        let bytes = dir_size(&root.join(&summary.run_id), &mut walk)?;
        sized.push((summary, bytes));
    }
    let mut remaining = sized.len();
    let mut total_bytes = sized
        .iter()
        .fold(0u64, |total, (_, bytes)| total.saturating_add(*bytes));
    let mut removed = 0usize;
    // Oldest first, deleting only while at least one configured limit is
    // exceeded. Status is empty for an in-progress pending manifest.
    for (summary, bytes) in sized.drain(..).rev() {
        let count_exceeded = keep > 0 && remaining > keep;
        let bytes_exceeded = max_bytes > 0 && total_bytes > max_bytes;
        if !count_exceeded && !bytes_exceeded {
            break;
        }
        if summary.status.is_empty() || summary.status == "running" {
            continue;
        }
        if delete_run(&summary.run_id).is_ok() {
            removed += 1;
            remaining = remaining.saturating_sub(1);
            total_bytes = total_bytes.saturating_sub(bytes);
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

    let abs_path = path.to_string_lossy().replace('\\', "/");

    // Sniff a small head for kind detection of extensionless files.
    let (head, size) = {
        use std::io::Read as _;
        let mut buf = vec![0u8; 512];
        let mut f = crate::safety::open_regular_file(&path)
            .map_err(|e| format!("Cannot open artifact: {e}"))?;
        let size = f
            .metadata()
            .map_err(|e| format!("Cannot stat artifact: {e}"))?
            .len();
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("Cannot read artifact: {e}"))?;
        buf.truncate(n);
        (buf, size)
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
            let limit = if rel_path == "context/document_bundle.json" {
                MAX_DOCUMENT_BUNDLE_BYTES
            } else {
                MAX_TEXT_BYTES
            };
            let (bytes, truncated) = read_at_most(&path, limit)?;
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
                cached_input_tokens: 700,
                cache_write_input_tokens: 100,
                ..Default::default()
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
        assert_eq!(s.output_tokens, 200);
        assert_eq!(s.cached_input_tokens, 700);
        assert_eq!(s.cache_write_input_tokens, 100);
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
    fn abrupt_run_recovery_builds_report_from_step_checkpoints() {
        let temp = tempfile::tempdir().unwrap();
        let mut writer = RunWriter::create_in(temp.path(), "abc123_run").unwrap();
        writer
            .add_text(
                "context/extracted_text.md",
                "Extracted text",
                "context",
                "paper",
            )
            .unwrap();
        writer
            .add_text(
                "context/orientation.json",
                "Orientation map",
                "context",
                r#"{"metadata":{"title":"Test"}}"#,
            )
            .unwrap();
        let run_dir = writer.dir().to_path_buf();
        let checkpoint_dir = run_dir.join("artifacts/checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        let output = crate::models::StepOutput {
            step_id: "technical".to_string(),
            step_label: "Technical".to_string(),
            phase: "parallel".to_string(),
            raw_text: "Recovered analysis".to_string(),
            ..Default::default()
        };
        fs::write(
            checkpoint_dir.join("0000_technical.json"),
            serde_json::to_vec_pretty(&output).unwrap(),
        )
        .unwrap();
        // Simulate an abort: Drop never gets the opportunity to mark the run
        // failed or persist its in-memory artifact index.
        std::mem::forget(writer);

        let manifest_json = fs::read_to_string(run_dir.join("manifest.json")).unwrap();
        let mut manifest: RunManifest = serde_json::from_str(&manifest_json).unwrap();
        assert_eq!(manifest.status, "running");
        assert!(recover_resumable_run_dir(&run_dir, &mut manifest).unwrap());

        assert_eq!(manifest.status, "interrupted");
        assert_eq!(manifest.step_count, 1);
        let report: crate::models::PipelineReport =
            serde_json::from_str(&fs::read_to_string(run_dir.join("report.json")).unwrap())
                .unwrap();
        assert_eq!(report.step_outputs.len(), 1);
        assert_eq!(report.step_outputs[0].raw_text, "Recovered analysis");
        assert!(run_dir.join("report.md").is_file());
    }

    #[test]
    fn cancelled_run_becomes_resumable_from_its_last_checkpoint() {
        let temp = tempfile::tempdir().unwrap();
        let mut writer = RunWriter::create_in(temp.path(), "def456_run").unwrap();
        writer
            .set_pending_meta(RunFinishMeta {
                input_path: "/papers/test.pdf".to_string(),
                input_mode: "document".to_string(),
                profile_id: "deep-review".to_string(),
                profile_name: "Deep Review".to_string(),
                ..Default::default()
            })
            .unwrap();
        writer
            .add_text(
                "context/extracted_text.md",
                "Extracted text",
                "context",
                "paper",
            )
            .unwrap();
        writer
            .add_text(
                "context/orientation.json",
                "Orientation map",
                "context",
                r#"{"metadata":{"title":"Test"}}"#,
            )
            .unwrap();
        let run_dir = writer.dir().to_path_buf();
        let checkpoint_dir = run_dir.join("artifacts/checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        let output = crate::models::StepOutput {
            step_id: "technical".to_string(),
            step_label: "Technical".to_string(),
            raw_text: "Durable work".to_string(),
            ..Default::default()
        };
        fs::write(
            checkpoint_dir.join("0000_technical.json"),
            serde_json::to_vec_pretty(&output).unwrap(),
        )
        .unwrap();

        let mut manifest = writer.current_manifest();
        manifest.status = "cancelled".to_string();
        write_manifest(&run_dir, &manifest).unwrap();
        std::mem::forget(writer);

        assert!(recover_resumable_run_dir(&run_dir, &mut manifest).unwrap());
        assert_eq!(manifest.status, "cancelled");
        assert_eq!(manifest.step_count, 1);
        assert_eq!(manifest.failed_steps, vec!["Run cancelled"]);
        assert!(run_has_resume_files(&run_dir, &manifest));
        let report: crate::models::PipelineReport =
            serde_json::from_str(&fs::read_to_string(run_dir.join("report.json")).unwrap())
                .unwrap();
        assert_eq!(report.step_outputs[0].raw_text, "Durable work");
        assert_eq!(report.failed_steps[0].step_label, "Run cancelled");
    }

    #[test]
    fn failed_run_without_a_captured_extraction_is_not_resumable() {
        let temp = tempfile::tempdir().unwrap();
        let writer = RunWriter::create_in(temp.path(), "no_context_run").unwrap();
        let run_dir = writer.dir().to_path_buf();
        let mut manifest = writer.current_manifest();
        manifest.status = "failed".to_string();
        write_manifest(&run_dir, &manifest).unwrap();
        std::mem::forget(writer);

        assert!(recover_resumable_run_dir(&run_dir, &mut manifest).is_err());
        assert!(!run_has_resume_files(&run_dir, &manifest));
        assert!(!run_dir.join("report.json").exists());
    }

    #[test]
    fn recovery_preserves_checkpointed_failure_identity() {
        let temp = tempfile::tempdir().unwrap();
        let mut writer = RunWriter::create_in(temp.path(), "failed_step_run").unwrap();
        writer
            .add_text(
                "context/extracted_text.md",
                "Extracted text",
                "context",
                "paper",
            )
            .unwrap();
        let run_dir = writer.dir().to_path_buf();
        let checkpoint_dir = run_dir.join("artifacts/checkpoints");
        fs::create_dir_all(&checkpoint_dir).unwrap();
        fs::write(
            checkpoint_dir.join("failure_empirical.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "failure": crate::models::StepFailure {
                    step_id: "empirical/gemini".to_string(),
                    step_label: "Empirical [Gemini]".to_string(),
                    error: "provider failed".to_string(),
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let mut manifest = writer.current_manifest();
        manifest.status = "failed".to_string();
        write_manifest(&run_dir, &manifest).unwrap();
        std::mem::forget(writer);

        assert!(recover_resumable_run_dir(&run_dir, &mut manifest).unwrap());
        let report: crate::models::PipelineReport =
            serde_json::from_str(&fs::read_to_string(run_dir.join("report.json")).unwrap())
                .unwrap();
        assert_eq!(report.failed_steps[0].step_id, "empirical/gemini");
        assert_eq!(manifest.failed_steps, vec!["Empirical [Gemini]"]);
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
    fn oversized_manifest_is_rejected_without_replacing_last_valid_copy() {
        let temp = tempfile::tempdir().unwrap();
        let mut writer = RunWriter::create_in(temp.path(), "bounded-manifest").unwrap();
        writer
            .meta
            .variables
            .insert("huge".to_string(), "x".repeat(MAX_MANIFEST_BYTES));
        assert!(writer.persist_current().is_err());

        let content =
            fs::read_to_string(temp.path().join("bounded-manifest/manifest.json")).unwrap();
        let manifest: RunManifest = serde_json::from_str(&content).unwrap();
        assert!(manifest.variables.is_empty());
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

    #[test]
    fn unlisted_artifact_cap_deletes_every_excess_file() {
        let temp = tempfile::tempdir().unwrap();
        let mut writer = RunWriter::create_in(temp.path(), "quota-run").unwrap();
        let artifacts = writer.dir().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        for index in 0..505 {
            fs::write(artifacts.join(format!("{index:04}.txt")), "x").unwrap();
        }

        assert_eq!(writer.register_unlisted("artifacts", "files"), 500);
        assert_eq!(fs::read_dir(&artifacts).unwrap().count(), 500);
    }

    #[test]
    #[cfg(unix)]
    fn unlisted_artifact_scan_removes_fifo_without_opening_it() {
        use std::os::unix::ffi::OsStrExt as _;

        let temp = tempfile::tempdir().unwrap();
        let mut writer = RunWriter::create_in(temp.path(), "fifo-run").unwrap();
        let artifacts = writer.dir().join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        let fifo = artifacts.join("blocked.pipe");
        let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);

        assert_eq!(writer.register_unlisted("artifacts", "files"), 0);
        assert!(!fifo.exists());
    }
}
