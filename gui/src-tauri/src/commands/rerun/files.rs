//! Bounded retained-context and report reads for rerun and comparison.

use super::super::MAX_RUN_CONTEXT_SIZE;
use crate::models::PipelineReport;
use std::io::Read;

pub(in crate::commands) fn read_run_file(run_id: &str, rel: &str) -> Result<String, String> {
    crate::runs::validate_run_id(run_id)?;
    read_run_file_in(&crate::runs::runs_dir()?.join(run_id), rel)
}

pub(in crate::commands) fn read_run_file_in(
    run_dir: &std::path::Path,
    rel: &str,
) -> Result<String, String> {
    if rel.is_empty()
        || std::path::Path::new(rel).is_absolute()
        || rel.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("Invalid run-relative path".to_string());
    }
    let run_dir = run_dir
        .canonicalize()
        .map_err(|_| "Run not found".to_string())?;
    let path = run_dir
        .join(rel)
        .canonicalize()
        .map_err(|e| format!("Cannot resolve {rel} from run: {e}"))?;
    if !path.starts_with(&run_dir) {
        return Err("Run-relative path resolves outside the run directory".to_string());
    }
    let file = crate::safety::open_regular_file(&path)
        .map_err(|e| format!("Cannot read {rel} from run: {e}"))?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_RUN_CONTEXT_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {rel} from run: {e}"))?;
    if bytes.len() as u64 > MAX_RUN_CONTEXT_SIZE {
        return Err(format!(
            "{rel} exceeds the {} MB run-context safety limit",
            MAX_RUN_CONTEXT_SIZE / 1024 / 1024
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("{rel} is not valid UTF-8: {e}"))
}

pub(in crate::commands) fn load_run_report(run_id: &str) -> Result<PipelineReport, String> {
    load_run_report_in(&crate::runs::runs_dir()?.join(run_id))
}

pub(in crate::commands) fn load_run_report_in(
    run_dir: &std::path::Path,
) -> Result<PipelineReport, String> {
    let json = read_run_file_in(run_dir, "report.json")
        .map_err(|_| "This run predates comparison support (no report.json).".to_string())?;
    let mut report: PipelineReport =
        serde_json::from_str(&json).map_err(|e| format!("Invalid report.json: {e}"))?;
    crate::findings::ensure_legacy_products(&mut report);
    Ok(report)
}
