//! Declared-output checks, immutable adoption, and observed dependency evidence.

use super::file_identity::{hash_regular_file, SNAPSHOT_BUDGET};
use super::types::ExecutionProfile;
use crate::workbench::research::papers::copy_immutable_within;
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
        let mut found_log = false;
        let mut clean = true;
        let mut logs = Vec::new();
        for relative in profile
            .outputs
            .iter()
            .filter(|path| path.to_ascii_lowercase().ends_with(".log"))
        {
            found_log = true;
            match read_log(&Path::new(&profile.cwd).join(relative)) {
                Some(log) => {
                    let errors = stata_log_errors(&log);
                    clean &= errors.is_empty();
                    logs.push(json!({"path":relative,"errors":errors}));
                }
                None => {
                    clean = false;
                    logs.push(json!({"path":relative,"readable":false}));
                }
            }
        }
        clean &= found_log;
        checks.push(json!({"check":"stataLogHasNoErrorCode","passed":clean,"logs":logs}));
        valid &= clean;
    }
    (json!({"passed":valid,"checks":checks}), valid)
}

const STATA_LOG_LIMIT: u64 = 256 * 1024 * 1024;

/// Read a log as lossy UTF-8 (Stata 13 and earlier write Latin-1 logs).
fn read_log(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(STATA_LOG_LIMIT + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= STATA_LOG_LIMIT).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// Stata return codes (`r(###);` on a line of their own) with the nearest
/// preceding message line. Batch logs repeat the code after "end of do-file",
/// so identical code/message pairs are reported once.
pub(super) fn stata_log_errors(log: &str) -> Vec<Value> {
    static STATA_ERROR_CODE: OnceLock<regex::Regex> = OnceLock::new();
    let error_code = STATA_ERROR_CODE.get_or_init(|| {
        regex::Regex::new(r"^r\(([0-9]+)\);\s*$").expect("static Stata error-code regex")
    });
    let mut errors: Vec<Value> = Vec::new();
    let mut previous: Option<&str> = None;
    for line in log.lines() {
        let trimmed = line.trim_end();
        if let Some(code) = error_code
            .captures(trimmed)
            .and_then(|captures| captures[1].parse::<u32>().ok())
        {
            let message = previous.map(|text| text.chars().take(300).collect::<String>());
            let entry = json!({"code":code,"message":message});
            if !errors.contains(&entry) && errors.len() < 20 {
                errors.push(entry);
            }
            continue;
        }
        let content = trimmed.trim();
        if !content.is_empty() && content != "end of do-file" && !content.starts_with("--Break--") {
            previous = Some(content);
        }
    }
    errors
}

/// Largest single output copied into private storage.
const OUTPUT_RETENTION_LIMIT: u64 = 200 * 1024 * 1024;

fn adopt_file(
    store: &Store,
    profile: &ExecutionProfile,
    source: &Path,
    hash: &str,
    size: u64,
    limit: u64,
) -> WorkbenchResult<String> {
    let suffix = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    let destination = store
        .root_path()
        .join("blobs")
        .join(format!("{hash}.{suffix}"));
    copy_immutable_within(source, &destination, hash, limit)?;
    let artifact_id = new_id("artifact")?;
    open_connection(store)?.execute("INSERT OR IGNORE INTO artifacts (id, workspace_id, content_hash, media_kind, size_bytes, origin, storage_reference, original_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, 'research_execution', ?6, ?7, ?8)", params![artifact_id, profile.workspace_id, hash, suffix, size as i64, destination.to_string_lossy(), source.to_string_lossy(), now()]).map_err(|error| WorkbenchError::storage("Failed to adopt execution artifact", error))?;
    open_connection(store)?
        .query_row(
            "SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind=?3",
            params![profile.workspace_id, hash, suffix],
            |row| row.get(0),
        )
        .map_err(|e| WorkbenchError::storage("Failed to resolve adopted artifact", e))
}

/// Keeps what it can and reports the rest, so that nothing about one file can
/// cost a finished run its logs and outcome. Each entry in `notRetained` says
/// why; it carries the hash whenever the file could be read as itself.
fn adopt_files(
    store: &Store,
    profile: &ExecutionProfile,
    files: &[String],
    execution_id: &str,
    limit: u64,
) -> Value {
    let mut adopted = Vec::new();
    let mut not_retained = Vec::new();
    for relative in files {
        let source = Path::new(&profile.cwd).join(relative);
        if !source.is_file() {
            continue;
        }
        let (hash, size) = match hash_regular_file(&source) {
            Ok(identity) => identity,
            Err(problem) => {
                not_retained.push(
                    json!({"path":relative,"reason":format!("This file {}", problem.reason())}),
                );
                continue;
            }
        };
        if size > limit {
            not_retained.push(json!({"path":relative,"contentHash":hash,"sizeBytes":size,"reason":format!("Larger than the {} MiB limit for kept files", limit / (1024 * 1024))}));
            continue;
        }
        match adopt_file(store, profile, &source, &hash, size, limit) {
            Ok(artifact_id) => adopted.push(json!({"path":relative,"contentHash":hash,"sizeBytes":size,"artifactId":artifact_id,"sourceExecutionId":execution_id})),
            Err(error) => not_retained.push(json!({"path":relative,"contentHash":hash,"sizeBytes":size,"reason":error.message})),
        }
    }
    let mut manifest = json!({"artifacts":adopted});
    if !not_retained.is_empty() {
        manifest["notRetained"] = json!(not_retained);
    }
    manifest
}

pub(super) fn adopt_execution_outputs(
    store: &Store,
    profile: &ExecutionProfile,
    execution_id: &str,
) -> Value {
    adopt_files(
        store,
        profile,
        &profile.outputs,
        execution_id,
        OUTPUT_RETENTION_LIMIT,
    )
}

pub(super) fn snapshot_execution_inputs(
    store: &Store,
    profile: &ExecutionProfile,
    execution_id: &str,
) -> Value {
    adopt_files(
        store,
        profile,
        &profile.inputs,
        execution_id,
        SNAPSHOT_BUDGET,
    )
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
