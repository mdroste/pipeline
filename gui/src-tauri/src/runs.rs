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

/// Text artifacts larger than this are truncated when read for display.
const MAX_TEXT_BYTES: usize = 1_000_000;
/// Images larger than this are not inlined (metadata only).
const MAX_IMAGE_BYTES: u64 = 10_000_000;

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

/// Accumulates artifacts for a run and writes the manifest at the end.
/// All writes are best-effort from the pipeline's perspective — callers log
/// failures but never fail the run because persistence failed.
pub struct RunWriter {
    dir: PathBuf,
    run_id: String,
    artifacts: Vec<ArtifactEntry>,
}

impl RunWriter {
    pub fn create(run_id: &str) -> Result<Self, String> {
        validate_run_id(run_id)?;
        let dir = runs_dir()?.join(run_id);
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create run dir: {e}"))?;
        Ok(Self {
            dir,
            run_id: run_id.to_string(),
            artifacts: Vec::new(),
        })
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn dir(&self) -> &Path {
        &self.dir
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
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create {rel_path} parent: {e}"))?;
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
        let bytes = fs::read(&path).map_err(|e| format!("Failed to read {rel_path}: {e}"))?;
        self.artifacts.push(ArtifactEntry {
            rel_path: rel_path.to_string(),
            label: label.to_string(),
            kind: detect_kind(rel_path, &bytes[..bytes.len().min(512)]).to_string(),
            bytes: bytes.len() as u64,
            sha256: short_sha256(&bytes),
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
        let known: std::collections::HashSet<String> =
            self.artifacts.iter().map(|a| a.rel_path.clone()).collect();
        let mut added = 0usize;
        let mut stack = vec![self.dir.join(subdir)];
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else { continue };
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
                if entry.metadata().map(|m| m.len() > MAX_UNLISTED_BYTES).unwrap_or(true) {
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&self.dir) else { continue };
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
                }
            }
        }
        added
    }

    /// Write manifest.json. Call once, last.
    pub fn finish(
        self,
        input_path: &str,
        input_mode: &str,
        profile_id: &str,
        profile_name: &str,
        provider: &str,
    ) -> Result<RunManifest, String> {
        let manifest = RunManifest {
            run_id: self.run_id,
            created: chrono::Local::now().to_rfc3339(),
            input_path: input_path.to_string(),
            input_mode: input_mode.to_string(),
            profile_id: profile_id.to_string(),
            profile_name: profile_name.to_string(),
            provider: provider.to_string(),
            artifacts: self.artifacts,
        };
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize manifest: {e}"))?;
        fs::write(self.dir.join("manifest.json"), json)
            .map_err(|e| format!("Failed to write manifest: {e}"))?;
        Ok(manifest)
    }
}

pub fn load_manifest(run_id: &str) -> Result<RunManifest, String> {
    validate_run_id(run_id)?;
    let path = runs_dir()?.join(run_id).join("manifest.json");
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read manifest: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid manifest: {e}"))
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
        let n = f.read(&mut buf).map_err(|e| format!("Cannot read artifact: {e}"))?;
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
            let bytes = fs::read(&path).map_err(|e| format!("Cannot read artifact: {e}"))?;
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
            let bytes = fs::read(&path).map_err(|e| format!("Cannot read artifact: {e}"))?;
            let truncated = bytes.len() > MAX_TEXT_BYTES;
            let slice = if truncated { &bytes[..MAX_TEXT_BYTES] } else { &bytes[..] };
            Ok(ArtifactContent {
                kind: kind.into(),
                bytes: size,
                text: Some(String::from_utf8_lossy(slice).into_owned()),
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
}
