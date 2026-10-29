//! Declared-output checks, immutable adoption, and observed dependency evidence.

use super::types::ExecutionProfile;
use crate::workbench::research::papers::{copy_immutable, hash_file};
use crate::workbench::research::{new_id, now, open_connection};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::params;
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

pub(in crate::workbench::research) fn validate_execution_outputs(
    profile: &ExecutionProfile,
    outcome: &str,
) -> (Value, bool) {
    let mut checks = Vec::new();
    let mut valid = outcome == "completed";
    for relative in &profile.outputs {
        let path = Path::new(&profile.cwd).join(relative);
        let present = path.is_file();
        checks.push(json!({"path":relative,"present":present}));
        valid &= present;
    }
    if profile.adapter == "latex" {
        let pdf = profile.outputs.iter().any(|path| {
            path.to_ascii_lowercase().ends_with(".pdf")
                && Path::new(&profile.cwd).join(path).is_file()
        });
        checks.push(json!({"check":"compiledPdf","passed":pdf}));
        valid &= pdf;
    }
    if profile.adapter == "stata" {
        static STATA_ERROR_CODE: OnceLock<regex::Regex> = OnceLock::new();
        let error_code = STATA_ERROR_CODE.get_or_init(|| {
            regex::Regex::new(r"(?m)^r\([0-9]+\);\s*$").expect("static Stata error-code regex")
        });
        let mut found_log = false;
        let mut clean = true;
        for relative in profile
            .outputs
            .iter()
            .filter(|path| path.to_ascii_lowercase().ends_with(".log"))
        {
            found_log = true;
            match fs::read_to_string(Path::new(&profile.cwd).join(relative)) {
                Ok(log) => clean &= !error_code.is_match(&log),
                Err(_) => clean = false,
            }
        }
        clean &= found_log;
        checks.push(json!({"check":"stataLogHasNoErrorCode","passed":clean}));
        valid &= clean;
    }
    (json!({"passed":valid,"checks":checks}), valid)
}

pub(super) fn adopt_execution_outputs(
    store: &Store,
    profile: &ExecutionProfile,
    execution_id: &str,
) -> WorkbenchResult<Value> {
    let mut adopted = Vec::new();
    for relative in &profile.outputs {
        let source = Path::new(&profile.cwd).join(relative);
        if !source.is_file() {
            continue;
        }
        let (hash, size) = hash_file(&source)?;
        let suffix = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin");
        let destination = store
            .root_path()
            .join("blobs")
            .join(format!("{hash}.{suffix}"));
        copy_immutable(&source, &destination, &hash)?;
        let artifact_id = new_id("artifact")?;
        open_connection(store)?.execute("INSERT OR IGNORE INTO artifacts (id, workspace_id, content_hash, media_kind, size_bytes, origin, storage_reference, original_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'research_execution', ?6, ?7, ?8)", params![artifact_id, profile.workspace_id, hash, suffix, size as i64, destination.to_string_lossy(), source.to_string_lossy(), now()]).map_err(|error| WorkbenchError::storage("Failed to adopt execution artifact", error))?;
        let artifact_id: String = open_connection(store)?.query_row("SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind=?3", params![profile.workspace_id,hash,suffix], |row|row.get(0)).map_err(|e|WorkbenchError::storage("Failed to resolve adopted artifact",e))?;
        adopted.push(json!({"path":relative,"contentHash":hash,"sizeBytes":size,"artifactId":artifact_id,"sourceExecutionId":execution_id}));
    }
    Ok(json!({"artifacts":adopted}))
}

/// TeX recorder observations are evidence of files opened on this run, not a closure.
pub(super) fn observed_dependencies(profile: &ExecutionProfile) -> Value {
    let mut paths = std::collections::BTreeSet::new();
    let mut truncated = false;
    for name in profile.outputs.iter().filter(|s| s.ends_with(".fls")) {
        let Ok(file) = fs::File::open(Path::new(&profile.cwd).join(name)) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file.take(512 * 1024 + 1).read_to_end(&mut bytes).is_err() {
            continue;
        }
        truncated |= bytes.len() > 512 * 1024;
        for line in String::from_utf8_lossy(&bytes[..bytes.len().min(512 * 1024)]).lines() {
            if let Some(path) = line.strip_prefix("INPUT ") {
                if paths.len() >= 1024 {
                    truncated = true;
                    break;
                }
                paths.insert(path.to_string());
            }
        }
    }
    json!({"method":"TeX recorder when declared as output","paths":paths,"truncated":truncated,"coverage":"observed on this run; not a complete dependency closure"})
}
