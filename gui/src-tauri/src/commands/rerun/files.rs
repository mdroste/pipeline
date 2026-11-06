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
    let limit = if rel == "report.json" {
        crate::runs::MAX_REPORT_BYTES as u64
    } else {
        MAX_RUN_CONTEXT_SIZE
    };
    let oversized = || {
        format!(
            "{rel} exceeds the {} MB run-context safety limit",
            limit >> 20
        )
    };
    if file
        .metadata()
        .map_err(|error| format!("Cannot inspect {rel}: {error}"))?
        .len()
        > limit
    {
        return Err(oversized());
    }
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {rel} from run: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err(oversized());
    }
    String::from_utf8(bytes).map_err(|e| format!("{rel} is not valid UTF-8: {e}"))
}

/// Check a recorded text artifact when the parent manifest supplies its
/// identity. Early and recovered runs may have only the bounded file itself.
pub(super) fn read_captured_text(
    parent: &crate::runs::RunManifest,
    rel: &str,
) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let text = read_run_file(&parent.run_id, rel)?;
    if let Some(entry) = parent.artifacts.iter().find(|e| e.rel_path == rel) {
        let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
        if entry.bytes != text.len() as u64
            || (!entry.sha256.is_empty() && entry.sha256 != digest[..16])
        {
            return Err(format!(
                "Captured artifact '{rel}' differs from its saved size or checksum."
            ));
        }
    }
    Ok(text)
}

/// The report a re-run starts from. Only a run with no report predates re-run
/// support: a report that exists keeps its real read error, and one that
/// recovery could not build reports why.
pub(super) fn read_parent_report(
    run_dir: &std::path::Path,
    recovery_error: Option<String>,
) -> Result<String, String> {
    read_run_file_in(run_dir, "report.json").map_err(|error| {
        if run_dir.join("report.json").exists() {
            return error;
        }
        recovery_error.unwrap_or_else(|| {
            "This run predates re-run support (no report.json). Re-run is only available for runs created after upgrading.".to_string()
        })
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parent_report_read_failure_keeps_its_real_error() {
        let run = tempfile::tempdir().unwrap();
        let report = run.path().join("report.json");
        std::fs::File::create(&report)
            .unwrap()
            .set_len(crate::runs::MAX_REPORT_BYTES as u64 + 1)
            .unwrap();
        let error = read_parent_report(run.path(), None).unwrap_err();
        assert!(error.contains("safety limit"), "{error}");

        std::fs::write(&report, [0xff, 0xfe]).unwrap();
        let error = read_parent_report(run.path(), None).unwrap_err();
        assert!(error.contains("not valid UTF-8"), "{error}");
    }

    #[test]
    fn a_missing_parent_report_says_why_it_is_missing() {
        let run = tempfile::tempdir().unwrap();
        let error = read_parent_report(run.path(), None).unwrap_err();
        assert!(error.contains("predates re-run support"), "{error}");
        let error = read_parent_report(run.path(), Some("No document".into())).unwrap_err();
        assert_eq!(error, "No document");
    }
}
