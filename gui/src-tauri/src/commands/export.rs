use super::*;

// --- File I/O ---

#[tauri::command]
pub async fn save_report_md(
    app: AppHandle,
    markdown: String,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let clean = output::normalize_math_delimiters(&output::clean_export_markdown(&markdown));
    let name = suggested_name.as_deref().unwrap_or("report.md");
    save_via_dialog(&app, name, "Markdown", "md", clean.into_bytes()).await
}

pub(super) const MAX_RUN_EXPORT_FILES: usize = 20_000;
pub(super) const MAX_RUN_EXPORT_DIRS: usize = 10_000;
pub(super) const MAX_RUN_EXPORT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRunArtifactsResult {
    pub exported_path: String,
    pub file_count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportMode {
    Shareable,
    Forensic,
    Custom,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportSelection {
    pub report: bool,
    pub verified_findings: bool,
    pub provenance: bool,
    pub workflow: bool,
    pub source_documents: bool,
    pub raw_responses: bool,
    pub logs: bool,
    pub supporting_artifacts: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPackageResult {
    pub exported_path: String,
    pub file_count: u64,
    pub bytes: u64,
    /// SHA-256 of `checksums.sha256`, which commits to every exported payload
    /// file and the export manifest without creating a circular checksum.
    pub checksum: String,
    pub mode: String,
    pub sensitivity: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExportFileRecord {
    pub(super) path: String,
    pub(super) bytes: u64,
    pub(super) sha256: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportManifest {
    schema_version: u32,
    run_id: String,
    created_at: String,
    mode: String,
    sensitivity: String,
    includes_source_material: bool,
    includes_raw_responses: bool,
    includes_logs: bool,
    includes_supporting_artifacts: bool,
    payload_checksum: String,
    files: Vec<ExportFileRecord>,
}

static RECENT_EXPORT_PATHS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::VecDeque<std::path::PathBuf>>,
> = std::sync::OnceLock::new();

pub(super) fn record_export_path(path: &std::path::Path) {
    let Ok(path) = path.canonicalize() else {
        return;
    };
    let paths = RECENT_EXPORT_PATHS
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()));
    if let Ok(mut paths) = paths.lock() {
        paths.retain(|existing| existing != &path);
        paths.push_front(path);
        paths.truncate(16);
    }
}

fn full_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn safe_export_relative_path(relative: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::Path::new(relative);
    if relative.is_empty()
        || relative.contains('\\')
        || !path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!("Invalid run artifact path '{relative}'"));
    }
    Ok(path.to_path_buf())
}

pub(super) fn copy_export_file(
    source_root: &std::path::Path,
    destination_root: &std::path::Path,
    relative: &str,
) -> Result<(), String> {
    let relative = safe_export_relative_path(relative)?;
    let source = source_root.join(&relative);
    let mut input = crate::safety::open_regular_file(&source)
        .map_err(|error| format!("Could not export '{}': {error}", relative.display()))?;
    let target = destination_root.join(&relative);
    let parent = target
        .parent()
        .ok_or_else(|| format!("Export path has no parent: {}", target.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create '{}': {error}", parent.display()))?;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)
        .map_err(|error| format!("Could not create '{}': {error}", target.display()))?;
    std::io::copy(&mut input, &mut output)
        .map_err(|error| format!("Could not copy '{}': {error}", relative.display()))?;
    Ok(())
}

fn write_export_json(
    destination_root: &std::path::Path,
    relative: &str,
    value: &impl serde::Serialize,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not serialize {relative}: {error}"))?;
    let target = destination_root.join(safe_export_relative_path(relative)?);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create export directory: {error}"))?;
    }
    std::fs::write(&target, bytes)
        .map_err(|error| format!("Could not write '{}': {error}", target.display()))
}

fn verified_status(status: &str) -> bool {
    matches!(status, "verified" | "verified_with_normalization")
}

pub(super) fn write_shareable_products(
    source: &std::path::Path,
    destination: &std::path::Path,
    manifest: &crate::runs::RunManifest,
    include_findings: bool,
    include_provenance: bool,
) -> Result<(), String> {
    let report_json = std::fs::read_to_string(source.join("report.json"))
        .map_err(|error| format!("Could not read structured report: {error}"))?;
    let report: crate::models::PipelineReport = serde_json::from_str(&report_json)
        .map_err(|error| format!("Could not parse structured report: {error}"))?;
    if include_findings {
        let mut findings = report.products.findings.unwrap_or_default();
        findings.findings = findings
            .findings
            .into_iter()
            .filter_map(|mut finding| {
                finding
                    .evidence
                    .retain(|evidence| verified_status(&evidence.verification_status));
                if verified_status(&finding.verification_status) || !finding.evidence.is_empty() {
                    for evidence in &mut finding.evidence {
                        // Internal artifact addresses are not portable and can
                        // reveal the run layout. Human citations remain.
                        evidence.artifact_path.clear();
                    }
                    Some(finding)
                } else {
                    None
                }
            })
            .collect();
        write_export_json(destination, "verified-findings.json", &findings)?;
        write_export_json(destination, "quality.json", &report.quality)?;
        let limitations = if report.quality.limitations.is_empty() {
            "# Limitations\n\nNo deterministic limitations were recorded. Shareable exports still omit unverified findings, source documents, raw model responses, and logs by default.\n".to_string()
        } else {
            format!(
                "# Limitations\n\n{}\n\nShareable exports omit unverified findings, source documents, raw model responses, and logs by default.\n",
                report
                    .quality
                    .limitations
                    .iter()
                    .map(|item| format!("- {item}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        std::fs::write(destination.join("limitations.md"), limitations)
            .map_err(|error| format!("Could not write limitations: {error}"))?;
    }
    if include_provenance {
        let provenance = serde_json::json!({
            "schemaVersion": 1,
            "runId": manifest.run_id,
            "created": manifest.created,
            "input": {
                "mode": manifest.input_mode,
                "interpretation": manifest.input_interpretation,
                "contentHash": manifest.input_identity.content_hash,
            },
            "profile": { "id": manifest.profile_id, "name": manifest.profile_name },
            "workflow": {
                "fingerprint": manifest.workflow_fingerprint,
                "specialistCatalogRevision": manifest.specialist_catalog_revision,
            },
            "provider": manifest.provider,
            "status": manifest.status,
            "durationSeconds": manifest.duration_secs,
            "usage": manifest.usage,
            "quality": report.quality,
            "redactions": [
                "absolute input paths", "run variables", "credential material",
                "raw model responses", "logs", "source documents"
            ],
        });
        write_export_json(destination, "provenance.json", &provenance)?;
    }
    Ok(())
}

fn custom_artifact_selected(
    artifact: &crate::runs::ArtifactEntry,
    selection: &ExportSelection,
) -> bool {
    let path = artifact.rel_path.as_str();
    if path == "report.md" || path == "findings.json" || path == "manifest.json" {
        return false;
    }
    if path == "context/workflow.json" {
        return selection.workflow;
    }
    if path == "report.json" || artifact.group == "step" || artifact.group == "agent_response" {
        return selection.raw_responses;
    }
    if path.starts_with("logs/") {
        return selection.logs;
    }
    if artifact.group == "document"
        || artifact.group == "pages"
        || artifact.group == "figures"
        || path.starts_with("context/source-evidence/")
        || path == crate::runs::DOCUMENT_TEXT_PATH
    {
        return selection.source_documents;
    }
    selection.supporting_artifacts
        && matches!(artifact.group.as_str(), "product" | "files" | "context")
}

pub(super) fn write_custom_payload(
    source: &std::path::Path,
    destination: &std::path::Path,
    manifest: &crate::runs::RunManifest,
    selection: &ExportSelection,
) -> Result<(), String> {
    if selection.report {
        copy_export_file(source, destination, "report.md")?;
    }
    if selection.verified_findings || selection.provenance {
        write_shareable_products(
            source,
            destination,
            manifest,
            selection.verified_findings,
            selection.provenance,
        )?;
    }
    for artifact in &manifest.artifacts {
        if custom_artifact_selected(artifact, selection) {
            copy_export_file(source, destination, &artifact.rel_path)?;
        }
    }
    if selection.source_documents {
        if let Some(pages) = &manifest.page_artifacts {
            for page in 1..=pages.count {
                let relative = format!(
                    "artifacts/pages/page-{page:0width$}.{}",
                    pages.extension,
                    width = pages.digit_width as usize
                );
                copy_export_file(source, destination, &relative)?;
            }
        }
    }
    Ok(())
}

pub(super) fn enumerate_export_files(
    root: &std::path::Path,
) -> Result<Vec<ExportFileRecord>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut paths = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .map_err(|error| format!("Could not inspect export: {error}"))?
        {
            let entry = entry.map_err(|error| format!("Could not inspect export: {error}"))?;
            let kind = entry
                .file_type()
                .map_err(|error| format!("Could not inspect export: {error}"))?;
            if kind.is_symlink() {
                return Err(format!(
                    "Export contains a symbolic link: {}",
                    entry.path().display()
                ));
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            use sha2::{Digest as _, Sha256};
            use std::io::Read as _;
            let mut file = crate::safety::open_regular_file(&path)
                .map_err(|error| format!("Could not checksum '{}': {error}", path.display()))?;
            let mut digest = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            let mut byte_count = 0u64;
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|error| format!("Could not checksum '{}': {error}", path.display()))?;
                if read == 0 {
                    break;
                }
                byte_count = byte_count.saturating_add(read as u64);
                digest.update(&buffer[..read]);
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "Export path escaped its package".to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            Ok(ExportFileRecord {
                path: relative,
                bytes: byte_count,
                sha256: format!("{:x}", digest.finalize()),
            })
        })
        .collect()
}

pub(super) fn finalize_export_metadata(
    root: &std::path::Path,
    run_id: &str,
    mode: ExportMode,
    selection: &ExportSelection,
) -> Result<(u64, u64, String, String), String> {
    let files = enumerate_export_files(root)?;
    let ledger = files
        .iter()
        .map(|file| format!("{}  {}", file.sha256, file.path))
        .collect::<Vec<_>>()
        .join("\n");
    let payload_checksum = full_sha256(ledger.as_bytes());
    let includes_source_material = mode == ExportMode::Forensic || selection.source_documents;
    let includes_raw_responses = mode == ExportMode::Forensic || selection.raw_responses;
    let includes_logs = mode == ExportMode::Forensic || selection.logs;
    let includes_supporting_artifacts =
        mode == ExportMode::Forensic || selection.supporting_artifacts;
    let sensitivity = if includes_source_material
        || includes_raw_responses
        || includes_logs
        || includes_supporting_artifacts
    {
        "sensitive"
    } else {
        "shareable"
    };
    let mode_label = match mode {
        ExportMode::Shareable => "shareable",
        ExportMode::Forensic => "forensic",
        ExportMode::Custom => "custom",
    };
    let manifest = ExportManifest {
        schema_version: 1,
        run_id: run_id.to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        mode: mode_label.to_string(),
        sensitivity: sensitivity.to_string(),
        includes_source_material,
        includes_raw_responses,
        includes_logs,
        includes_supporting_artifacts,
        payload_checksum,
        files,
    };
    write_export_json(root, "export-manifest.json", &manifest)?;
    let all_files = enumerate_export_files(root)?;
    let checksums = format!(
        "{}\n",
        all_files
            .iter()
            .map(|file| format!("{}  {}", file.sha256, file.path))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::fs::write(root.join("checksums.sha256"), checksums.as_bytes())
        .map_err(|error| format!("Could not write checksums: {error}"))?;
    let checksum = full_sha256(checksums.as_bytes());
    let final_files = enumerate_export_files(root)?;
    let bytes = final_files.iter().map(|file| file.bytes).sum();
    Ok((
        final_files.len() as u64,
        bytes,
        checksum,
        sensitivity.to_string(),
    ))
}

pub(super) fn create_export_stage(
    destination: &std::path::Path,
) -> Result<tempfile::TempDir, String> {
    std::fs::create_dir_all(destination).map_err(|error| {
        format!(
            "Failed to create export destination '{}': {error}",
            destination.display()
        )
    })?;
    if !destination.is_dir() {
        return Err(format!(
            "Export destination '{}' is not a directory",
            destination.display()
        ));
    }
    tempfile::Builder::new()
        .prefix(".pipeline-export-staging-")
        .tempdir_in(destination)
        .map_err(|error| format!("Failed to create export staging directory: {error}"))
}

pub(super) fn finish_export_stage(
    stage: &tempfile::TempDir,
    destination: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, String> {
    for suffix in 0..1_000usize {
        let candidate_name = if suffix == 0 {
            name.to_string()
        } else {
            format!("{name}-{}", suffix + 1)
        };
        let candidate = destination.join(candidate_name);
        if candidate.exists() {
            continue;
        }
        match std::fs::rename(stage.path(), &candidate) {
            Ok(()) => return Ok(candidate),
            Err(_) if candidate.exists() => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to finalize export '{}': {error}",
                    candidate.display()
                ));
            }
        }
    }
    Err("Could not choose a unique export directory name".to_string())
}

pub(super) fn copy_export_tree(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(u64, u64), String> {
    let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
    let mut file_count = 0u64;
    let mut directory_count = 1usize;
    let mut total_bytes = 0u64;
    while let Some((source_dir, destination_dir)) = pending.pop() {
        std::fs::create_dir_all(&destination_dir).map_err(|error| {
            format!(
                "Failed to create export directory '{}': {error}",
                destination_dir.display()
            )
        })?;
        let entries = std::fs::read_dir(&source_dir).map_err(|error| {
            format!(
                "Failed to read run directory '{}': {error}",
                source_dir.display()
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("Failed to read run entry: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Failed to inspect run entry: {error}"))?;
            if file_type.is_symlink() {
                return Err(format!(
                    "Run export refused symbolic link '{}'",
                    entry.path().display()
                ));
            }
            let target = destination_dir.join(entry.file_name());
            if file_type.is_dir() {
                directory_count = directory_count.saturating_add(1);
                if directory_count > MAX_RUN_EXPORT_DIRS {
                    return Err(format!(
                        "Run export exceeds the {MAX_RUN_EXPORT_DIRS}-directory safety limit"
                    ));
                }
                pending.push((entry.path(), target));
                continue;
            }
            if !file_type.is_file() {
                return Err(format!(
                    "Run export refused non-regular file '{}'",
                    entry.path().display()
                ));
            }
            file_count = file_count.saturating_add(1);
            if file_count as usize > MAX_RUN_EXPORT_FILES {
                return Err(format!(
                    "Run export exceeds the {MAX_RUN_EXPORT_FILES}-file safety limit"
                ));
            }
            let mut input = crate::safety::open_regular_file(&entry.path())?;
            let size = input
                .metadata()
                .map_err(|error| format!("Failed to inspect run artifact: {error}"))?
                .len();
            if total_bytes.saturating_add(size) > MAX_RUN_EXPORT_BYTES {
                return Err(format!(
                    "Run export exceeds the {} GB safety limit",
                    MAX_RUN_EXPORT_BYTES / 1024 / 1024 / 1024
                ));
            }
            let mut output = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
                .map_err(|error| {
                    format!(
                        "Failed to create exported artifact '{}': {error}",
                        target.display()
                    )
                })?;
            let remaining = MAX_RUN_EXPORT_BYTES.saturating_sub(total_bytes);
            let mut limited = std::io::Read::take(&mut input, remaining.saturating_add(1));
            let copied = std::io::copy(&mut limited, &mut output)
                .map_err(|error| format!("Failed to copy run artifact: {error}"))?;
            if copied != size || copied > remaining {
                return Err(format!(
                    "Run artifact '{}' changed or exceeded limits during export",
                    entry.path().display()
                ));
            }
            total_bytes = total_bytes.saturating_add(copied);
        }
    }
    Ok((file_count, total_bytes))
}

/// Export one durable run as a complete, self-contained directory. All files
/// are copied into a hidden staging directory first, then atomically renamed
/// to a new unique child of the user-selected destination.
#[tauri::command]
pub async fn export_run_artifacts(
    run_id: String,
    destination: String,
) -> Result<ExportRunArtifactsResult, String> {
    let result = export_run_package(
        run_id,
        destination,
        ExportMode::Forensic,
        ExportSelection::default(),
    )
    .await?;
    Ok(ExportRunArtifactsResult {
        exported_path: result.exported_path,
        file_count: result.file_count,
        bytes: result.bytes,
    })
}

/// Export a durable run under an explicit disclosure contract. Shareable is
/// intentionally allowlist-only; forensic is intentionally complete and
/// labelled sensitive; custom copies exactly the selected artifact classes.
#[tauri::command]
pub async fn export_run_package(
    run_id: String,
    destination: String,
    mode: ExportMode,
    selection: ExportSelection,
) -> Result<ExportPackageResult, String> {
    tokio::task::spawn_blocking(move || {
        crate::runs::validate_run_id(&run_id)?;
        let manifest = crate::runs::load_manifest(&run_id)?;
        if manifest.status == "running" {
            return Err("Wait for this run to finish before exporting it".to_string());
        }
        let runs_root = crate::runs::runs_dir()?
            .canonicalize()
            .map_err(|error| format!("Failed to resolve run storage: {error}"))?;
        let source = runs_root
            .join(&run_id)
            .canonicalize()
            .map_err(|error| format!("Failed to resolve run '{run_id}': {error}"))?;
        if !source.starts_with(&runs_root) || !source.is_dir() {
            return Err("Run export source is invalid".to_string());
        }
        let destination = std::path::PathBuf::from(destination);
        std::fs::create_dir_all(&destination)
            .map_err(|error| format!("Failed to create export destination: {error}"))?;
        let destination = destination
            .canonicalize()
            .map_err(|error| format!("Failed to resolve export destination: {error}"))?;
        if destination.starts_with(&runs_root) {
            return Err(
                "Choose an export destination outside Pipeline's internal run storage".to_string(),
            );
        }
        let stage = create_export_stage(&destination)?;
        match mode {
            ExportMode::Shareable => {
                copy_export_file(&source, stage.path(), "report.md")?;
                write_shareable_products(&source, stage.path(), &manifest, true, true)?;
            }
            ExportMode::Forensic => {
                copy_export_tree(&source, stage.path())?;
            }
            ExportMode::Custom => {
                if !selection.report
                    && !selection.verified_findings
                    && !selection.provenance
                    && !selection.workflow
                    && !selection.source_documents
                    && !selection.raw_responses
                    && !selection.logs
                    && !selection.supporting_artifacts
                {
                    return Err("Select at least one item for a custom export".to_string());
                }
                write_custom_payload(&source, stage.path(), &manifest, &selection)?;
            }
        }
        let (file_count, bytes, checksum, sensitivity) =
            finalize_export_metadata(stage.path(), &run_id, mode, &selection)?;
        let mode_label = match mode {
            ExportMode::Shareable => "shareable",
            ExportMode::Forensic => "forensic",
            ExportMode::Custom => "custom",
        };
        let exported = finish_export_stage(
            &stage,
            &destination,
            &format!("pipeline-{mode_label}-{run_id}"),
        )?;
        record_export_path(&exported);
        Ok(ExportPackageResult {
            exported_path: exported.to_string_lossy().to_string(),
            file_count,
            bytes,
            checksum,
            mode: mode_label.to_string(),
            sensitivity,
        })
    })
    .await
    .map_err(|error| format!("Run export task failed: {error}"))?
}

pub(super) fn validated_recent_export(path: &str) -> Result<std::path::PathBuf, String> {
    let canonical = std::path::Path::new(path)
        .canonicalize()
        .map_err(|error| format!("Could not resolve the exported package: {error}"))?;
    let paths = RECENT_EXPORT_PATHS
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()));
    let paths = paths
        .lock()
        .map_err(|_| "Recent export registry is unavailable".to_string())?;
    if !paths.iter().any(|recent| recent == &canonical) {
        return Err("Pipeline can reveal only a package exported in this app session".to_string());
    }
    Ok(canonical)
}

#[tauri::command]
pub fn reveal_export_in_folder(path: String) -> Result<(), String> {
    let path = validated_recent_export(&path)?;
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(&path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(format!("/select,{}", path.display()));
        command
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path.parent().unwrap_or(&path));
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not reveal the exported package: {error}"))
}

/// Legacy in-memory export retained for older frontend builds. It now stages
/// into a new subdirectory, so it cannot overwrite an earlier export or leave
/// a partially written result at the final path.
#[tauri::command]
pub async fn save_all_artifacts(
    dir: String,
    markdown: String,
    extracted_text: String,
    report: crate::models::PipelineReport,
) -> Result<(), String> {
    let destination = std::path::Path::new(&dir);
    let stage = create_export_stage(destination)?;
    let base = stage.path();

    // Final report
    let clean_markdown =
        output::normalize_math_delimiters(&output::clean_export_markdown(&markdown));
    std::fs::write(base.join("report.md"), &clean_markdown)
        .map_err(|e| format!("Failed to write report.md: {e}"))?;

    // Canonical readable document. The argument name remains stable for older
    // frontend builds that still invoke this compatibility command.
    std::fs::write(base.join("document.md"), &extracted_text)
        .map_err(|e| format!("Failed to write document.md: {e}"))?;

    // Orientation map
    let orient_json = serde_json::to_string_pretty(&report.orientation)
        .map_err(|e| format!("Failed to serialize orientation: {e}"))?;
    std::fs::write(base.join("orientation.json"), &orient_json)
        .map_err(|e| format!("Failed to write orientation.json: {e}"))?;

    if let Some(findings) = report.products.findings.as_ref() {
        let findings_json = serde_json::to_string_pretty(findings)
            .map_err(|e| format!("Failed to serialize findings: {e}"))?;
        std::fs::write(base.join("findings.json"), findings_json)
            .map_err(|e| format!("Failed to write findings.json: {e}"))?;
    }

    // Individual step outputs
    let steps_dir = base.join("steps");
    std::fs::create_dir_all(&steps_dir)
        .map_err(|e| format!("Failed to create steps directory: {e}"))?;

    let outputs = report.all_outputs();
    for (i, output) in outputs.iter().enumerate() {
        let slug = output
            .step_id
            .replace('/', "_")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let (filename, contents) = if output.structured_json {
            (
                format!("{:02}_{}.json", i + 1, slug),
                output.raw_text.clone(),
            )
        } else {
            let header = format!(
                "# {}\n\n**Phase**: {} · **Agent**: {}\n\n---\n\n",
                output.step_label,
                output.phase,
                if output.agent.is_empty() {
                    "default"
                } else {
                    &output.agent
                },
            );
            (
                format!("{:02}_{}.md", i + 1, slug),
                format!("{}{}", header, output.raw_text),
            )
        };
        std::fs::write(steps_dir.join(&filename), contents)
            .map_err(|e| format!("Failed to write {filename}: {e}"))?;
    }

    finish_export_stage(&stage, destination, "pipeline-core-export")?;
    Ok(())
}

/// Stash the last export path so we can clean it up on the next export.
pub(super) static LAST_EXPORT_PATH: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

pub(crate) fn cleanup_print_export() {
    if let Ok(mut previous) = LAST_EXPORT_PATH.lock() {
        if let Some(path) = previous.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(crate) fn cleanup_stale_print_exports() {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(24 * 60 * 60))
        .unwrap_or(std::time::UNIX_EPOCH);
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten().take(1_000) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("pipeline_report_") || !name.ends_with(".html") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file()
            && metadata
                .modified()
                .map(|time| time < cutoff)
                .unwrap_or(false)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub(super) fn offline_katex_css() -> Result<String, String> {
    use base64::Engine as _;

    const KATEX_CSS: &str = include_str!("../../../node_modules/katex/dist/katex.min.css");
    const FONTS: &[(&str, &[u8])] = &[
        (
            "KaTeX_AMS-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_AMS-Regular.woff2"),
        ),
        (
            "KaTeX_Caligraphic-Bold",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Caligraphic-Bold.woff2"),
        ),
        (
            "KaTeX_Caligraphic-Regular",
            include_bytes!(
                "../../../node_modules/katex/dist/fonts/KaTeX_Caligraphic-Regular.woff2"
            ),
        ),
        (
            "KaTeX_Fraktur-Bold",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Fraktur-Bold.woff2"),
        ),
        (
            "KaTeX_Fraktur-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Fraktur-Regular.woff2"),
        ),
        (
            "KaTeX_Main-Bold",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Main-Bold.woff2"),
        ),
        (
            "KaTeX_Main-BoldItalic",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Main-BoldItalic.woff2"),
        ),
        (
            "KaTeX_Main-Italic",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Main-Italic.woff2"),
        ),
        (
            "KaTeX_Main-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Main-Regular.woff2"),
        ),
        (
            "KaTeX_Math-BoldItalic",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Math-BoldItalic.woff2"),
        ),
        (
            "KaTeX_Math-Italic",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Math-Italic.woff2"),
        ),
        (
            "KaTeX_SansSerif-Bold",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_SansSerif-Bold.woff2"),
        ),
        (
            "KaTeX_SansSerif-Italic",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_SansSerif-Italic.woff2"),
        ),
        (
            "KaTeX_SansSerif-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_SansSerif-Regular.woff2"),
        ),
        (
            "KaTeX_Script-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Script-Regular.woff2"),
        ),
        (
            "KaTeX_Size1-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Size1-Regular.woff2"),
        ),
        (
            "KaTeX_Size2-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Size2-Regular.woff2"),
        ),
        (
            "KaTeX_Size3-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Size3-Regular.woff2"),
        ),
        (
            "KaTeX_Size4-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Size4-Regular.woff2"),
        ),
        (
            "KaTeX_Typewriter-Regular",
            include_bytes!("../../../node_modules/katex/dist/fonts/KaTeX_Typewriter-Regular.woff2"),
        ),
    ];

    let mut css = KATEX_CSS.to_string();
    for (name, bytes) in FONTS {
        let original = format!(
            "src:url(fonts/{name}.woff2) format(\"woff2\"),\
             url(fonts/{name}.woff) format(\"woff\"),\
             url(fonts/{name}.ttf) format(\"truetype\")"
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let embedded = format!("src:url(\"data:font/woff2;base64,{encoded}\") format(\"woff2\")");
        if !css.contains(&original) {
            return Err(format!(
                "Bundled KaTeX CSS does not match embedded font {name}"
            ));
        }
        css = css.replace(&original, &embedded);
    }
    if css.contains("url(fonts/") || css.contains("http://") || css.contains("https://") {
        return Err("Bundled KaTeX CSS still contains an external font reference".to_string());
    }
    Ok(css)
}

#[derive(Debug)]
pub(super) struct ProtectedPrintMath {
    token: String,
    latex: String,
    display: bool,
}

pub(super) fn escaped_at(bytes: &[u8], index: usize) -> bool {
    let mut slashes = 0usize;
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        slashes += 1;
        cursor -= 1;
    }
    slashes % 2 == 1
}

pub(super) fn math_close(source: &str, mut index: usize, delimiter_len: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
            continue;
        }
        if bytes[index] == b'$' && !escaped_at(bytes, index) {
            if delimiter_len == 2 {
                if bytes.get(index + 1) == Some(&b'$') {
                    return Some(index);
                }
            } else if bytes.get(index + 1) != Some(&b'$') {
                return Some(index);
            }
        }
        let character = source[index..].chars().next()?;
        index += character.len_utf8();
    }
    None
}

pub(super) fn protect_math_chunk(
    source: &str,
    token_base: &str,
    protected: &mut Vec<ProtectedPrintMath>,
) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0usize;
    while index < bytes.len() {
        // Copy code spans byte-for-byte. KaTeX auto-render ignores code tags,
        // and dollar signs in examples must never become equations.
        if bytes[index] == b'`' {
            let ticks = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            let marker = "`".repeat(ticks);
            if let Some(relative_end) = source[index + ticks..].find(&marker) {
                let end = index + ticks + relative_end + ticks;
                output.push_str(&source[index..end]);
                index = end;
                continue;
            }
        }

        if bytes[index] == b'$' && !escaped_at(bytes, index) {
            let delimiter_len = usize::from(bytes.get(index + 1) == Some(&b'$')) + 1;
            let content_start = index + delimiter_len;
            if let Some(close) = math_close(source, content_start, delimiter_len) {
                let latex = &source[content_start..close];
                // Inline `$…$` must have non-whitespace ends and stay on one
                // line without crossing a code span: math_close scans raw
                // bytes, so "from $5 to $8 … `$x$`" would otherwise pair a
                // currency `$` with a later paragraph's or code span's `$`
                // and swallow the prose in between as an equation.
                let valid_inline_spacing = delimiter_len == 2
                    || (!latex.chars().next().is_some_and(char::is_whitespace)
                        && !latex.chars().next_back().is_some_and(char::is_whitespace)
                        && !latex.contains('\n')
                        && !latex.contains('`'));
                if !latex.trim().is_empty() && valid_inline_spacing {
                    let token = format!("{token_base}{}END", protected.len());
                    protected.push(ProtectedPrintMath {
                        token: token.clone(),
                        latex: latex.to_string(),
                        display: delimiter_len == 2,
                    });
                    output.push_str(&token);
                    index = close + delimiter_len;
                    continue;
                }
            }
        }

        let character = source[index..]
            .chars()
            .next()
            .expect("index remains on a UTF-8 boundary");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

/// Replace math with alphanumeric placeholders before pulldown-cmark sees it.
/// Otherwise `_`, `*`, and backslashes inside LaTeX can be consumed as Markdown
/// emphasis/escapes before KaTeX's auto-render pass runs.
pub(super) fn protect_print_math(markdown: &str) -> (String, Vec<ProtectedPrintMath>) {
    let mut discriminator = 0usize;
    let token_base = loop {
        let candidate = format!("PIPELINEMATHPLACEHOLDER{discriminator}X");
        if !markdown.contains(&candidate) {
            break candidate;
        }
        discriminator += 1;
    };

    let mut output = String::with_capacity(markdown.len());
    let mut prose = String::new();
    let mut protected = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let flush_prose =
        |output: &mut String, prose: &mut String, protected: &mut Vec<ProtectedPrintMath>| {
            if !prose.is_empty() {
                output.push_str(&protect_math_chunk(prose, &token_base, protected));
                prose.clear();
            }
        };

    for line in markdown.split_inclusive('\n') {
        if let Some((marker, count)) = output::fence_marker(line) {
            flush_prose(&mut output, &mut prose, &mut protected);
            if let Some((open_marker, open_count)) = fence {
                if marker == open_marker && count >= open_count {
                    fence = None;
                }
            } else {
                fence = Some((marker, count));
            }
            output.push_str(line);
        } else if fence.is_some() {
            output.push_str(line);
        } else {
            prose.push_str(line);
        }
    }
    flush_prose(&mut output, &mut prose, &mut protected);
    (output, protected)
}

pub(super) fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) fn restore_print_math(mut html: String, protected: &[ProtectedPrintMath]) -> String {
    for item in protected {
        // Restore with backslash delimiters: the client-side renderMathInElement
        // pass deliberately does not scan for `$`, so unprotected currency
        // amounts in prose ("tariffs rose from $5 to $8") stay literal text
        // instead of being typeset as an equation in the printed report.
        let expression = if item.display {
            format!("\\[{}\\]", escape_html_text(&item.latex))
        } else {
            format!("\\({}\\)", escape_html_text(&item.latex))
        };
        if item.display {
            let paragraph = format!("<p>{}</p>", item.token);
            if html.contains(&paragraph) {
                html = html.replace(
                    &paragraph,
                    &format!("<div class=\"pipeline-display-equation\">{expression}</div>"),
                );
                continue;
            }
        }
        html = html.replace(&item.token, &expression);
    }
    html
}

/// Match the numbered issue treatment used by ReportViewer. Pulldown-cmark
/// renders `**#N. Title**` as a plain strong paragraph, so the standalone
/// export needs this small structural pass before it can share the viewer's
/// visual hierarchy.
pub(super) fn style_print_issue_headers(html: String) -> String {
    let issue_header = regex::Regex::new(r#"(?s)<p><strong>#([0-9]+)\.\s*(.*?)</strong></p>"#)
        .expect("print issue-header regex is valid");
    issue_header
        .replace_all(&html, |captures: &regex::Captures<'_>| {
            format!(
                "<div class=\"comment-header\"><span class=\"comment-num\">{}</span><span class=\"comment-title\">{}</span></div>",
                &captures[1], &captures[2]
            )
        })
        .into_owned()
}

pub(super) fn render_print_markdown(markdown: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};

    let markdown = output::normalize_math_delimiters(&output::clean_export_markdown(markdown));
    let (markdown, protected_math) = protect_print_math(&markdown);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&markdown, options);
    let mut html_body = String::new();
    html::push_html(&mut html_body, parser);

    // Sanitize HTML to strip <script>, event handlers, and other XSS vectors
    // that could be injected via LLM output (e.g. prompt injection in paper
    // text). Images keep their alt text but lose `src`, so opening the
    // temporary print document cannot fetch remote or local subresources.
    let html_body = ammonia::Builder::default()
        .rm_tag_attributes("img", &["src"])
        .clean(&html_body)
        .to_string();
    restore_print_math(style_print_issue_headers(html_body), &protected_math)
}

/// Keep only the compact opening metadata. Older callers may still send the
/// former detailed provenance section, so discard that suffix defensively.
pub(super) fn print_provenance_masthead(html: String) -> String {
    const DETAILS_HEADING: &str = "<h2>Run provenance</h2>";
    match html.split_once(DETAILS_HEADING) {
        Some((masthead, _)) => masthead.to_string(),
        None => html,
    }
}

pub(super) fn build_print_report_html(
    markdown: &str,
    provenance_markdown: Option<&str>,
) -> Result<String, String> {
    let report_html = render_print_markdown(markdown);
    let provenance_html = provenance_markdown
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(render_print_markdown)
        .map(print_provenance_masthead);

    // KaTeX code, CSS, and every WOFF2 face are compiled into this one HTML
    // document. The print path therefore performs no network or file fetches.
    const KATEX_JS: &str = include_str!("../../../node_modules/katex/dist/katex.min.js");
    const AUTO_RENDER_JS: &str =
        include_str!("../../../node_modules/katex/dist/contrib/auto-render.min.js");
    let katex_css = offline_katex_css()?;

    let mut html_doc = String::with_capacity(
        katex_css.len()
            + KATEX_JS.len()
            + AUTO_RENDER_JS.len()
            + report_html.len()
            + provenance_html.as_ref().map_or(0, String::len)
            + 12_288,
    );
    html_doc.push_str("<!DOCTYPE html>\n<html><head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Pipeline Report</title>\n<style>\n");
    html_doc.push_str(&katex_css);
    html_doc.push_str("\n</style>\n<script>\n");
    html_doc.push_str(KATEX_JS);
    html_doc.push_str("\n</script>\n<script>\n");
    html_doc.push_str(AUTO_RENDER_JS);
    html_doc.push_str(concat!(
        "\n</script>\n<style>\n",
        ":root { --ink: #20242a; --muted: #66707a; --navy: #18364d; --rule: #d7d9d8; --paper: #fffefb; --canvas: #e9e7e2; color-scheme: light; font-family: \"Iowan Old Style\", \"Palatino Linotype\", \"Book Antiqua\", Palatino, Georgia, \"Times New Roman\", serif; }\n",
        "* { box-sizing: border-box; }\n",
        "html { background: var(--canvas); }\n",
        "body { margin: 0; color: var(--ink); font-size: 16px; line-height: 1.68; -webkit-font-smoothing: antialiased; font-kerning: normal; text-rendering: optimizeLegibility; }\n",
        ".report-document { width: min(50rem, calc(100% - 2rem)); margin: 2.75rem auto; padding: 4rem 4.5rem 4.75rem; background: var(--paper); border: 1px solid rgba(41, 47, 54, 0.12); border-radius: 0.2rem; box-shadow: 0 22px 55px rgba(35, 38, 41, 0.13); }\n",
        "h1, h2, h3 { break-after: avoid; break-inside: avoid; }\n",
        "h1 { margin: 0 0 1rem; color: var(--ink); font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Arial, sans-serif; font-size: 1.7rem; font-weight: 680; letter-spacing: -0.025em; line-height: 1.2; }\n",
        "h2 { margin: 2.9rem 0 1.15rem; padding-bottom: 0.45rem; border-bottom: 1px solid var(--rule); color: var(--navy); font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Arial, sans-serif; font-size: 0.78rem; font-weight: 750; letter-spacing: 0.105em; line-height: 1.4; text-transform: uppercase; }\n",
        "h3 { margin: 2rem 0 0.75rem; color: var(--ink); font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Arial, sans-serif; font-size: 0.98rem; font-weight: 700; line-height: 1.4; }\n",
        "p { margin: 0 0 0.82rem; orphans: 3; widows: 3; }\n",
        "ul, ol { margin: 0 0 0.9rem; padding-left: 1.35rem; }\n",
        "li { margin-bottom: 0.32rem; padding-left: 0.15rem; line-height: 1.58; }\n",
        "li > p { margin-bottom: 0.35rem; }\n",
        "a { color: var(--navy); text-decoration-color: #9aabb6; text-underline-offset: 0.16em; }\n",
        "strong { color: #171a1e; font-weight: 700; }\n",
        "hr { margin: 2.25rem 0; border: 0; border-top: 1px solid var(--rule); }\n",
        ".report-masthead { margin-bottom: 2.25rem; padding: 0 0 1.25rem; border-bottom: 1px solid var(--rule); }\n",
        ".report-masthead > h1 { margin: 0 0 0.8rem; color: var(--ink); font-size: 0.92rem; font-weight: 720; letter-spacing: -0.01em; text-transform: none; }\n",
        ".report-masthead > p { display: grid; grid-template-columns: 7.15rem minmax(0, 1fr); column-gap: 0.65rem; margin: 0.24rem 0; color: #4f5961; font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Arial, sans-serif; font-size: 0.73rem; line-height: 1.45; }\n",
        ".report-masthead > p strong { color: var(--muted); font-size: 0.67rem; font-weight: 700; letter-spacing: 0.025em; }\n",
        ".report-body > h1:first-child, .report-body > h2:first-child, .report-body > h3:first-child { margin-top: 0; }\n",
        ".comment-header { display: grid; grid-template-columns: 1.75rem minmax(0, 1fr); align-items: start; column-gap: 0.8rem; margin: 2.25rem 0 0.72rem; padding: 0.85rem 0 0; border-top: 1px solid var(--rule); break-after: avoid; break-inside: avoid; }\n",
        ".comment-num { display: inline-flex; width: 1.55rem; height: 1.55rem; align-items: center; justify-content: center; border-radius: 999px; background: var(--navy); color: #fff; font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Arial, sans-serif; font-size: 0.68rem; font-weight: 750; font-variant-numeric: tabular-nums; line-height: 1; }\n",
        ".comment-title { padding-top: 0.05rem; color: var(--ink); font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Arial, sans-serif; font-size: 0.98rem; font-weight: 700; line-height: 1.42; }\n",
        "blockquote { margin: 1.25rem 0 1.35rem; padding: 0.65rem 0 0.65rem 1.1rem; border-left: 2px solid #9cabb4; color: #4f5961; font-size: 0.95em; font-style: italic; break-inside: avoid; }\n",
        "code { padding: 0.12rem 0.32rem; border-radius: 0.2rem; background: #f1f0ed; color: #30373d; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.8em; overflow-wrap: anywhere; }\n",
        "pre { margin: 1.15rem 0; padding: 1rem 1.1rem; overflow: auto; border: 1px solid #deddd9; border-radius: 0.25rem; background: #f7f6f3; white-space: pre-wrap; overflow-wrap: anywhere; break-inside: avoid; }\n",
        "pre code { padding: 0; background: transparent; font-size: 0.8125rem; }\n",
        "table { width: 100%; margin: 1.35rem 0 1.6rem; border-collapse: collapse; font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Arial, sans-serif; font-size: 0.79rem; line-height: 1.45; font-variant-numeric: tabular-nums; }\n",
        "th, td { padding: 0.48rem 0.52rem; border: 0; border-bottom: 1px solid #dfe1e1; text-align: left; vertical-align: top; overflow-wrap: anywhere; }\n",
        "th { border-top: 1.5px solid #59636b; border-bottom-color: #8a9298; background: transparent; color: #566069; font-size: 0.67rem; font-weight: 750; letter-spacing: 0.065em; text-transform: uppercase; }\n",
        "tbody tr:last-child td { border-bottom: 1.5px solid #59636b; }\n",
        "thead { display: table-header-group; }\n",
        "tr { break-inside: avoid; }\n",
        ".pipeline-display-equation, .katex-display { break-before: avoid; break-inside: avoid; }\n",
        ".katex { font-size: 1em; }\n",
        ".katex-display { margin: 1.15rem 0; overflow: visible; }\n",
        "img { max-width: 100%; height: auto; }\n",
        "@page { margin: 0 0 0.28in; }\n",
        "@page { @bottom-center { content: counter(page) \" / \" counter(pages); color: #7b838a; font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Arial, sans-serif; font-size: 8pt; font-variant-numeric: tabular-nums; } }\n",
        "@media print {\n",
        "  html, body { background: #fff; }\n",
        "  body { color: var(--ink); font-size: 10.4pt; line-height: 1.6; print-color-adjust: exact; -webkit-print-color-adjust: exact; }\n",
        "  .report-document { width: 100%; max-width: none; margin: 0; padding: 0.72in 0.82in 0.7in; border: 0; border-radius: 0; box-shadow: none; -webkit-box-decoration-break: clone; box-decoration-break: clone; }\n",
        "  h1 { font-size: 21pt; }\n",
        "  h2 { margin-top: 2.2rem; font-size: 8.3pt; }\n",
        "  h3 { margin-top: 1.5rem; font-size: 10.7pt; }\n",
        "  .report-masthead { margin-bottom: 1.65rem; padding-bottom: 1rem; }\n",
        "  .comment-header { margin-top: 1.55rem; padding-top: 0.65rem; }\n",
        "  .comment-num { width: 1.42rem; height: 1.42rem; font-size: 6.8pt; }\n",
        "  .comment-title { font-size: 10.2pt; }\n",
        "  .report-body p { text-align: justify; hyphens: auto; }\n",
        "  a { color: inherit; text-decoration: none; }\n",
        "  pre, blockquote { break-inside: avoid; }\n",
        "}\n",
        "@media (max-width: 640px) { .report-document { width: 100%; margin: 0; padding: 2.25rem 1.35rem 3rem; border: 0; border-radius: 0; box-shadow: none; } .report-masthead > p { grid-template-columns: 1fr; row-gap: 0.05rem; margin-top: 0.45rem; } }\n",
        "</style>\n</head><body>\n<article class=\"report-document\">\n",
    ));
    if let Some(provenance_html) = provenance_html.as_ref() {
        html_doc.push_str("<header class=\"report-masthead\">\n");
        html_doc.push_str(provenance_html);
        html_doc.push_str("</header>\n");
    }
    html_doc.push_str("<main class=\"report-body\">\n");
    html_doc.push_str(&report_html);
    html_doc.push_str("</main>\n");
    html_doc.push_str("</article>");
    // Rendering is synchronous, but embedded webfonts are not. Wait for the
    // font set before the final paint and print dialog.
    html_doc.push_str(concat!(
        "\n<script>",
        // No `$`/`$$` delimiters here: all legitimate dollar-delimited math was
        // already tokenized server-side by protect_print_math and restored as
        // \( \) / \[ \]; a bare `$` reaching this pass is prose currency.
        "renderMathInElement(document.body,{delimiters:[",
        "{left:'\\\\(',right:'\\\\)',display:false},",
        "{left:'\\\\[',right:'\\\\]',display:true}",
        "],throwOnError:false,strict:'ignore',trust:false,macros:{",
        "'\\\\bm':'\\\\boldsymbol','\\\\mathbbm':'\\\\mathbb','\\\\mathds':'\\\\mathbb',",
        "'\\\\R':'\\\\mathbb{R}','\\\\N':'\\\\mathbb{N}','\\\\Z':'\\\\mathbb{Z}',",
        "'\\\\Q':'\\\\mathbb{Q}','\\\\E':'\\\\mathbb{E}',",
        "'\\\\Var':'\\\\operatorname{Var}','\\\\Cov':'\\\\operatorname{Cov}',",
        "'\\\\diag':'\\\\operatorname{diag}'}});",
        "var fontsReady=(document.fonts&&document.fonts.ready)?document.fonts.ready:Promise.resolve();",
        "fontsReady.then(function(){requestAnimationFrame(function(){window.print();});});",
        "</script>\n</body></html>",
    ));
    Ok(html_doc)
}

#[tauri::command]
pub async fn print_report_html(
    markdown: String,
    provenance_markdown: Option<String>,
) -> Result<(), String> {
    let html_doc = build_print_report_html(&markdown, provenance_markdown.as_deref())?;
    // Clean up previous export file
    cleanup_print_export();

    let mut tmp = tempfile::Builder::new()
        .prefix("pipeline_report_")
        .suffix(".html")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    tmp.write_all(html_doc.as_bytes())
        .map_err(|e| format!("Failed to write HTML: {e}"))?;
    tmp.flush().map_err(|e| format!("Failed to flush: {e}"))?;

    let path = tmp.into_temp_path();
    let path_buf = path
        .keep()
        .map_err(|e| format!("Failed to persist temp file: {e}"))?;

    if let Ok(mut prev) = LAST_EXPORT_PATH.lock() {
        *prev = Some(path_buf.clone());
    }

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("--")
        .arg(&path_buf)
        .spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(&path_buf)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg("--")
        .arg(&path_buf)
        .spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;

    Ok(())
}
