//! Deterministic package enumeration, checksums and disclosure manifest.
use super::files::write_export_json;
use super::types::{ExportMode, ExportSelection};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::commands) struct ExportFileRecord {
    pub(in crate::commands) path: String,
    pub(in crate::commands) bytes: u64,
    pub(in crate::commands) sha256: String,
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

fn full_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub(in crate::commands) fn enumerate_export_files(
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

pub(in crate::commands) fn finalize_export_metadata(
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
