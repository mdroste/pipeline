//! Selective project exchange: a `.pwex` package of one Workspace's selected
//! research objects with a previewed dependency closure, importable into a
//! nonempty store under a source namespace with ID remapping, duplicate
//! detection, and reviewable conflicts. Imports never carry the sender's
//! acceptance; see `authority`.
//!
//! The package never contains conversations, credentials, native runtime state,
//! execution profiles or authorizations, or declared raw execution inputs. It
//! stays readable without Pipeline: `README.md` summarizes every object and
//! `objects/<kind>/<id>.json` holds the structured records.
// Public operations retain this compatibility facade; private owners name their dependencies.
use super::archive;
use super::archive::safe_archive_name;
use super::now;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

mod authority;
mod conflicts;
mod export;
mod import_apply;
mod import_plan;
mod package;
mod selection;
mod types;

pub use conflicts::{list_conflicts, resolve_conflict};
pub use export::{export_exchange, preview_exchange};
pub use import_apply::import_exchange;
pub use import_plan::preview_import;
pub use package::inspect_exchange;
pub use types::{
    BlobEntry, ExchangeConflict, ExchangeInspection, ExchangeManifest, ExchangePreview,
    ExchangeReport, ExchangeSelection, ExchangeTarget, ExportExchangeRequest, ExternalReference,
    ImportExchangeRequest, ImportPreview, ImportReport, ObjectDecision, ObjectEntry,
    EXCHANGE_FORMAT, EXCHANGE_FORMAT_VERSION, RECORD_KINDS,
};

fn err(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::storage("Project exchange failed", e)
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_file(path: &Path) -> WorkbenchResult<(String, u64)> {
    let mut file = File::open(path).map_err(err)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(err)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}
fn random_id(prefix: &str) -> WorkbenchResult<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(err)?;
    Ok(format!(
        "{prefix}_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
fn sort_json(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            for (key, mut child) in entries {
                sort_json(&mut child);
                object.insert(key, child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(sort_json),
        _ => {}
    }
}
/// A stable per-store namespace so two machines never share an ID namespace.
pub fn instance_id(store: &Store) -> WorkbenchResult<String> {
    let path = store.root_path().join("instance.id");
    if let Ok(existing) = fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if trimmed.len() == 32 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(trimmed.to_string());
        }
    }
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(err)?;
    let id = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    fs::write(&path, &id).map_err(err)?;
    Ok(id)
}
fn validate_path(path: &str, extension: &str) -> WorkbenchResult<PathBuf> {
    let p = Path::new(path);
    if !p.is_absolute() || p.extension().and_then(|e| e.to_str()) != Some(extension) {
        return Err(WorkbenchError::invalid(format!(
            "Choose an absolute .{extension} path"
        )));
    }
    Ok(p.to_path_buf())
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}
fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .map(|x| match x {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}
fn json_text(v: &Value, key: &str) -> Option<String> {
    match v.get(key) {
        None | Some(Value::Null) => None,
        Some(x) => Some(x.to_string()),
    }
}

mod fingerprint;
pub use fingerprint::fingerprint;

#[cfg(test)]
mod tests;
