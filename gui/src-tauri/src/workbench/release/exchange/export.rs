//! Exchange preview, manifest/README assembly, and package publication.
use super::selection::{collect, Package};
use super::types::{
    BlobEntry, ExchangeManifest, ExchangePreview, ExchangeReport, ExchangeSelection,
    ExportExchangeRequest, ObjectEntry, EXCHANGE_FORMAT, EXCHANGE_FORMAT_VERSION,
    MAX_PACKAGE_BYTES,
};
use super::{err, fingerprint, hash_file, instance_id, now, safe_archive_name, validate_path};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use zip::write::SimpleFileOptions;

fn readme(manifest: &ExchangeManifest, pkg: &Package) -> String {
    let mut md = format!(
        "# Research exchange: {}\n\nExported {} from Workspace `{}` (namespace `{}`). Format {} v{}.\n\n",
        manifest.workspace_name,
        manifest.created_at,
        manifest.workspace_id,
        &manifest.source_namespace[..12.min(manifest.source_namespace.len())],
        manifest.format,
        manifest.format_version
    );
    md.push_str("This package is readable without Pipeline: every object is a JSON file under `objects/<kind>/`, immutable text and artifacts are under `blobs/`, and this file lists them. Statuses and dispositions are the exporting researcher's; model assessments are labeled as such and are not verification.\n\n");
    let mut by_kind: BTreeMap<&str, Vec<&ObjectEntry>> = BTreeMap::new();
    for o in &manifest.objects {
        by_kind.entry(o.kind.as_str()).or_default().push(o);
    }
    md.push_str("## Contents\n\n| Kind | Count |\n|---|---|\n");
    for (kind, items) in &by_kind {
        md.push_str(&format!("| {kind} | {} |\n", items.len()));
    }
    md.push_str(&format!(
        "| immutable blobs | {} |\n\n",
        manifest.blobs.len()
    ));
    for (kind, items) in &by_kind {
        md.push_str(&format!("## {}\n\n", kind.replace('_', " ")));
        if *kind == "response" {
            md.push_str("| Comment | Disposition | Category | Intended response | Links |\n|---|---|---|---|---|\n");
            for o in items {
                let v = &pkg.objects[&(o.kind.clone(), o.id.clone())];
                let d = &v["body"]["decision"];
                let links = [
                    "taskId",
                    "manuscriptRevisionId",
                    "applicationId",
                    "executionId",
                ]
                .iter()
                .filter_map(|k| d[*k].as_str().map(|s| format!("{k}={s}")))
                .collect::<Vec<_>>()
                .join(", ");
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    d["number"].as_str().unwrap_or("?"),
                    d["disposition"].as_str().unwrap_or(""),
                    d["category"].as_str().unwrap_or(""),
                    d["intendedResponse"]
                        .as_str()
                        .unwrap_or("")
                        .replace('|', "\\|")
                        .replace('\n', " "),
                    links
                ));
            }
            md.push('\n');
            continue;
        }
        for o in items {
            let v = &pkg.objects[&(o.kind.clone(), o.id.clone())];
            let status = ["status", "state", "outcome", "disposition"]
                .iter()
                .find_map(|k| {
                    v.get(*k)
                        .or_else(|| v["body"].get(*k))
                        .and_then(Value::as_str)
                })
                .unwrap_or("");
            md.push_str(&format!(
                "- `{}` {}{}\n",
                o.id,
                o.title.replace('\n', " "),
                if status.is_empty() {
                    String::new()
                } else {
                    format!(" — {status}")
                }
            ));
        }
        md.push('\n');
    }
    md.push_str("## Excluded by default\n\n");
    for e in &manifest.exclusions {
        md.push_str(&format!("- {e}\n"));
    }
    if !manifest.external_references.is_empty() {
        md.push_str("\n## External references not packaged\n\n");
        for e in manifest.external_references.iter().take(500) {
            md.push_str(&format!("- {} `{}`: {}\n", e.kind, e.id, e.reason));
        }
        if manifest.external_references.len() > 500 {
            md.push_str("- … further references are listed in manifest.json\n");
        }
    }
    md.push_str("\n## Limitations\n\n");
    for l in &manifest.limitations {
        md.push_str(&format!("- {l}\n"));
    }
    md.push_str(&format!("\n{}\n", manifest.reproducibility));
    md
}

fn exclusions() -> Vec<String> {
    vec![
        "Conversations, transcripts, drafts and native thread bindings".into(),
        "Managed sign-in, Codex home and any credential".into(),
        "Execution profiles, host authorizations and other machine-specific executable settings"
            .into(),
        "Harness presets, Workspace settings and recipe runs".into(),
        "Declared execution inputs and unselected or raw datasets (hashes remain in receipts)"
            .into(),
        "Performance samples, evaluations and Workflow runs".into(),
    ]
}
fn manifest_for(
    store: &Store,
    ws: &str,
    selection: &ExchangeSelection,
    pkg: &Package,
) -> WorkbenchResult<ExchangeManifest> {
    let workspace = store.workspace(ws)?;
    let objects = pkg
        .objects
        .iter()
        .map(|((kind, id), value)| ObjectEntry {
            kind: kind.clone(),
            id: id.clone(),
            fingerprint: fingerprint(value),
            title: pkg
                .titles
                .get(&(kind.clone(), id.clone()))
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    let blobs = pkg
        .blobs
        .iter()
        .map(|(name, (_, hash, size, role))| BlobEntry {
            archive_path: name.clone(),
            sha256: hash.clone(),
            size_bytes: *size,
            role: role.clone(),
        })
        .collect();
    let mut limitations: Vec<String> = pkg.limitations.iter().cloned().collect();
    if pkg.external.iter().any(|e| e.kind == "execution_input") {
        limitations.push("Declared execution inputs (possibly large or restricted datasets) are not included; results are not reproducible from this package alone.".into());
    }
    Ok(ExchangeManifest {
        format: EXCHANGE_FORMAT.into(),
        format_version: EXCHANGE_FORMAT_VERSION,
        store_schema_version: CURRENT_SCHEMA_VERSION,
        created_at: now(),
        source_namespace: format!("{}:{}", instance_id(store)?, ws),
        workspace_id: ws.into(),
        workspace_name: workspace.name,
        workspace_root: workspace.root,
        selection: selection.clone(),
        objects,
        blobs,
        exclusions: exclusions(),
        external_references: pkg.external.clone(),
        limitations,
        reproducibility: "Reproducibility statement: receipts, hashes and outputs are evidence of what ran on the exporting machine. Nothing in this package reruns automatically, and missing inputs or excluded artifacts are listed above rather than assumed present.".into(),
    })
}
pub fn preview_exchange(
    store: &Store,
    ws: &str,
    selection: &ExchangeSelection,
) -> WorkbenchResult<ExchangePreview> {
    store.workspace(ws)?;
    let pkg = collect(store, ws, selection)?;
    let manifest = manifest_for(store, ws, selection, &pkg)?;
    Ok(ExchangePreview {
        objects: manifest.objects,
        blob_count: manifest.blobs.len(),
        blob_bytes: manifest.blobs.iter().map(|b| b.size_bytes).sum(),
        exclusions: manifest.exclusions,
        external_references: manifest.external_references,
        limitations: manifest.limitations,
    })
}
pub fn export_exchange(
    store: &Store,
    request: ExportExchangeRequest,
) -> WorkbenchResult<ExchangeReport> {
    let target = validate_path(&request.path, "pwex")?;
    if target.starts_with(store.root_path()) {
        return Err(WorkbenchError::invalid(
            "Choose a package destination outside Workspace private storage",
        ));
    }
    let ws = &request.workspace_id;
    store.workspace(ws)?;
    let pkg = collect(store, ws, &request.selection)?;
    if pkg.objects.is_empty() {
        return Err(WorkbenchError::invalid("The selection contains no objects"));
    }
    let manifest = manifest_for(store, ws, &request.selection, &pkg)?;
    let parent = target
        .parent()
        .ok_or_else(|| WorkbenchError::invalid("Package destination has no parent directory"))?;
    fs::create_dir_all(parent).map_err(err)?;
    let temporary = tempfile::Builder::new()
        .prefix(".project-exchange-")
        .suffix(".pwex.partial")
        .tempfile_in(parent)
        .map_err(err)?;
    let file = temporary.reopen().map_err(err)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(err)?;
    zip.start_file("manifest.json", options).map_err(err)?;
    zip.write_all(manifest_json.as_bytes()).map_err(err)?;
    zip.start_file("README.md", options).map_err(err)?;
    zip.write_all(readme(&manifest, &pkg).as_bytes())
        .map_err(err)?;
    let mut expanded = manifest_json.len() as u64;
    for ((kind, id), value) in &pkg.objects {
        if !safe_archive_name(id) {
            return Err(WorkbenchError::invalid("Object ID is not a safe file name"));
        }
        let encoded = serde_json::to_string_pretty(value).map_err(err)?;
        expanded = expanded.saturating_add(encoded.len() as u64);
        zip.start_file(format!("objects/{kind}/{id}.json"), options)
            .map_err(err)?;
        zip.write_all(encoded.as_bytes()).map_err(err)?;
    }
    for (name, (path, hash, size, _)) in &pkg.blobs {
        expanded = expanded.saturating_add(*size);
        if expanded > MAX_PACKAGE_BYTES {
            return Err(WorkbenchError::invalid("Expanded package exceeds 2 GiB"));
        }
        let (actual, _) = hash_file(path)?;
        if &actual != hash {
            return Err(WorkbenchError::invalid(
                "An immutable blob changed during export; retry",
            ));
        }
        zip.start_file(name, options).map_err(err)?;
        std::io::copy(&mut File::open(path).map_err(err)?, &mut zip).map_err(err)?;
    }
    let file = zip.finish().map_err(err)?;
    file.sync_all().map_err(err)?;
    drop(file);
    temporary
        .persist(&target)
        .map_err(|e| WorkbenchError::storage("Failed to publish package", e.error))?;
    let (package_hash, bytes) = hash_file(&target)?;
    Ok(ExchangeReport {
        path: target.to_string_lossy().into_owned(),
        bytes,
        package_hash,
        object_count: manifest.objects.len(),
        blob_count: manifest.blobs.len(),
        exclusions: manifest.exclusions,
        external_references: manifest.external_references,
        limitations: manifest.limitations,
    })
}
