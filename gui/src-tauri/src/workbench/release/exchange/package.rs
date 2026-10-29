//! Bounded selective-package extraction and inspection.
use super::types::{
    ExchangeInspection, ExchangeManifest, EXCHANGE_FORMAT, EXCHANGE_FORMAT_VERSION, MAX_ENTRIES,
    MAX_ENTRY_BYTES, MAX_OBJECTS, MAX_PACKAGE_BYTES,
};
use super::{err, hash_file, instance_id, safe_archive_name, validate_path};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::path::PathBuf;

pub(super) struct Extracted {
    _temp: tempfile::TempDir,
    pub(super) root: PathBuf,
    pub(super) manifest: ExchangeManifest,
    pub(super) package_hash: String,
}
pub(super) fn extract(path: &str) -> WorkbenchResult<Extracted> {
    let source = validate_path(path, "pwex")?;
    let (package_hash, size) = hash_file(&source)?;
    if size > MAX_PACKAGE_BYTES {
        return Err(WorkbenchError::invalid("Package exceeds 2 GiB"));
    }
    let temp = tempfile::tempdir().map_err(err)?;
    let mut archive = zip::ZipArchive::new(File::open(&source).map_err(err)?)
        .map_err(|e| WorkbenchError::invalid(format!("Invalid exchange package: {e}")))?;
    if archive.len() > MAX_ENTRIES {
        return Err(WorkbenchError::invalid("Package contains too many entries"));
    }
    let mut total = 0u64;
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| WorkbenchError::invalid(format!("Invalid package entry: {e}")))?;
        let name = entry
            .name()
            .map_err(|error| {
                WorkbenchError::invalid(format!("Invalid archive entry name: {error}"))
            })?
            .into_owned();
        if !safe_archive_name(&name)
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(WorkbenchError::invalid(
                "Package contains a traversal, directory, or symbolic-link entry",
            ));
        }
        if !names.insert(name.clone()) {
            return Err(WorkbenchError::invalid(
                "Package contains duplicate entries",
            ));
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(WorkbenchError::invalid("Package entry exceeds 256 MiB"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_PACKAGE_BYTES {
            return Err(WorkbenchError::invalid("Expanded package exceeds 2 GiB"));
        }
        let destination = temp.path().join(&name);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(err)?;
        }
        let mut output = File::create(&destination).map_err(err)?;
        super::archive::copy_entry(&mut entry, &mut output)?;
    }
    let manifest: ExchangeManifest = serde_json::from_reader(
        File::open(temp.path().join("manifest.json"))
            .map_err(|_| WorkbenchError::invalid("Package manifest is missing"))?,
    )
    .map_err(|e| WorkbenchError::invalid(format!("Invalid package manifest: {e}")))?;
    if manifest.format != EXCHANGE_FORMAT || manifest.format_version != EXCHANGE_FORMAT_VERSION {
        return Err(WorkbenchError::invalid(
            "This is not a supported project exchange package",
        ));
    }
    if manifest.store_schema_version > CURRENT_SCHEMA_VERSION {
        return Err(WorkbenchError::invalid(
            "Package was written by a newer Pipeline; update before importing",
        ));
    }
    if manifest.objects.len() > MAX_OBJECTS {
        return Err(WorkbenchError::invalid("Package lists too many objects"));
    }
    let mut seen = HashSet::new();
    for o in &manifest.objects {
        if !safe_archive_name(&o.id) || !seen.insert((o.kind.clone(), o.id.clone())) {
            return Err(WorkbenchError::invalid(
                "Package manifest lists unsafe or duplicate objects",
            ));
        }
        let path = temp
            .path()
            .join(format!("objects/{}/{}.json", o.kind, o.id));
        if !path.is_file() {
            return Err(WorkbenchError::invalid(format!(
                "Package is missing object {}/{}",
                o.kind, o.id
            )));
        }
        if !names.contains(&format!("objects/{}/{}.json", o.kind, o.id)) {
            return Err(WorkbenchError::invalid(
                "Package object list disagrees with entries",
            ));
        }
    }
    let object_entries = names.iter().filter(|n| n.starts_with("objects/")).count();
    if object_entries != manifest.objects.len() {
        return Err(WorkbenchError::invalid(
            "Package contains objects absent from its manifest",
        ));
    }
    for b in &manifest.blobs {
        if !safe_archive_name(&b.archive_path) || !b.archive_path.starts_with("blobs/") {
            return Err(WorkbenchError::invalid("Package blob path is unsafe"));
        }
        let path = temp.path().join(&b.archive_path);
        if !path.is_file() {
            return Err(WorkbenchError::invalid(format!(
                "Package is missing blob {}",
                b.archive_path
            )));
        }
        let (hash, size) = hash_file(&path)?;
        if hash != b.sha256 || size != b.size_bytes {
            return Err(WorkbenchError::invalid(format!(
                "Blob {} failed its hash check",
                b.archive_path
            )));
        }
    }
    let blob_entries = names.iter().filter(|n| n.starts_with("blobs/")).count();
    if blob_entries != manifest.blobs.len() {
        return Err(WorkbenchError::invalid(
            "Package contains blobs absent from its manifest",
        ));
    }
    Ok(Extracted {
        root: temp.path().to_path_buf(),
        _temp: temp,
        manifest,
        package_hash,
    })
}

pub fn inspect_exchange(store: &Store, path: &str) -> WorkbenchResult<ExchangeInspection> {
    let x = extract(path)?;
    let mut counts = BTreeMap::new();
    for o in &x.manifest.objects {
        *counts.entry(o.kind.clone()).or_insert(0) += 1;
    }
    let own = x
        .manifest
        .source_namespace
        .starts_with(&format!("{}:", instance_id(store)?));
    Ok(ExchangeInspection {
        path: path.into(),
        package_hash: x.package_hash,
        workspace_name: x.manifest.workspace_name,
        source_namespace: x.manifest.source_namespace,
        created_at: x.manifest.created_at,
        workspace_root: x.manifest.workspace_root,
        counts,
        blob_count: x.manifest.blobs.len(),
        blob_bytes: x.manifest.blobs.iter().map(|b| b.size_bytes).sum(),
        exclusions: x.manifest.exclusions,
        external_references: x.manifest.external_references.len(),
        limitations: x.manifest.limitations,
        own_export: own,
    })
}
