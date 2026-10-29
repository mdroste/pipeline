//! Run writer and its finalization/Drop lifetime.
use super::persistence;
use super::{
    detect_kind, inspect_file, new_run_id, read_utf8_at_most, runs_dir, short_sha256,
    validate_run_id, write_manifest, ArtifactEntry, PageArtifactIndex, RunFinishMeta, RunManifest,
    CURRENT_ARTIFACT_SCHEMA_VERSION, MAX_MANIFEST_ARTIFACTS, MAX_MANIFEST_BYTES,
    MAX_SOURCE_EVIDENCE_FILES, MAX_SOURCE_EVIDENCE_FILE_BYTES, MAX_SOURCE_EVIDENCE_TOTAL_BYTES,
};
use crate::models::RunProducts;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Accumulates artifacts for a run and writes the manifest at the end.
/// Required artifacts are published atomically. Callers must propagate write
/// failures rather than reporting a run as complete without durable results.
pub struct RunWriter {
    pub(super) dir: PathBuf,
    pub(super) run_id: String,
    pub(super) artifacts: Vec<ArtifactEntry>,
    pub(super) page_artifacts: Option<PageArtifactIndex>,
    pub(super) created: String,
    pub(super) meta: RunFinishMeta,
    pub(super) finished: bool,
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

    pub(crate) fn create_in(base: &Path, run_id: &str) -> Result<Self, String> {
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

    pub(super) fn add_bytes(
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

    pub(super) fn current_manifest(&self) -> RunManifest {
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

    pub(super) fn persist_current(&self) -> Result<RunManifest, String> {
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
