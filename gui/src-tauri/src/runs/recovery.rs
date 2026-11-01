//! Recovery from retained responses and durable logical-step receipts.
use super::{
    open_run_lock, read_utf8_at_most, recovery_failure, register_recovered_artifact,
    register_recovered_directory, run_lock_path, runs_dir, validate_run_id, write_manifest,
    write_text_atomic, RunManifest, CURRENT_ARTIFACT_SCHEMA_VERSION, DOCUMENT_TEXT_PATH,
    LEGACY_EXTRACTED_TEXT_PATH, MAX_MANIFEST_BYTES, MAX_REPORT_BYTES,
};
use std::fs;
use std::path::Path;

/// Legacy identity marker for a run whose permanent deletion started in an
/// older Pipeline release. Current user and retention deletion moves the whole
/// run to recoverable Trash; startup recovery still honors old tombstones.
pub(super) const DELETE_TOMBSTONE: &str = "manifest.deleting.json";

fn resumable_status(status: &str) -> bool {
    matches!(status, "partial" | "failed" | "cancelled" | "interrupted")
}

fn captured_workflow_config(dir: &Path) -> Option<crate::pipeline_config::PipelineConfig> {
    let json = read_utf8_at_most(
        &dir.join("context").join("workflow.json"),
        MAX_REPORT_BYTES,
        "Workflow snapshot",
    )
    .ok()?;
    crate::pipeline_config::parse_workflow_document_strict(&json)
        .ok()
        .map(|workflow| workflow.config)
}

/// Resolve the exact document text used to restart a run. Current manifests
/// require the canonical `document.md`; version-0 manifests require the old
/// raw extraction because their `document.md` may contain a bundle preamble.
pub(crate) fn captured_document_rel_path(
    dir: &Path,
    manifest: &RunManifest,
) -> Result<&'static str, String> {
    if manifest.artifact_schema_version >= CURRENT_ARTIFACT_SCHEMA_VERSION {
        crate::safety::open_regular_file(&dir.join(DOCUMENT_TEXT_PATH))
            .map_err(|_| "Incomplete run has no canonical document to resume from".to_string())?;
        return Ok(DOCUMENT_TEXT_PATH);
    }
    if crate::safety::open_regular_file(&dir.join(LEGACY_EXTRACTED_TEXT_PATH)).is_ok() {
        return Ok(LEGACY_EXTRACTED_TEXT_PATH);
    }
    Err("Legacy run has no exact captured extraction to resume from".to_string())
}

pub(super) fn run_has_resume_files(dir: &Path, manifest: &RunManifest) -> bool {
    if !resumable_status(&manifest.status) || captured_document_rel_path(dir, manifest).is_err() {
        return false;
    }
    read_utf8_at_most(&dir.join("report.json"), MAX_REPORT_BYTES, "Run report")
        .ok()
        .and_then(|json| serde_json::from_str::<crate::models::PipelineReport>(&json).ok())
        .is_some()
}

/// Turn the durable pieces of an unfinished run into the same structured
/// report consumed by the normal resume path. Returns true only when recovery
/// wrote a report; completed/unsupported runs are left untouched.
pub(super) fn recover_resumable_run_dir(
    dir: &Path,
    manifest: &mut RunManifest,
) -> Result<bool, String> {
    if !matches!(
        manifest.status.as_str(),
        "running" | "partial" | "failed" | "cancelled" | "interrupted" | "done" | "degraded"
    ) {
        return Ok(false);
    }
    let artifact_count_before = manifest.artifacts.len();
    register_recovered_directory(manifest, dir, "artifacts/agent-responses", "agent_response");
    let recovered_response_index = manifest.artifacts.len() > artifact_count_before;
    // A completed run deleted its step checkpoints at finalization, so the
    // rebuild below could only produce an empty report — and would replace a
    // possibly-good report.md along with the unreadable report.json (for
    // example one over the read cap, or damaged by an external tool). Leave
    // completed runs untouched rather than destroying delivered output, which
    // also means their reports need no reading here.
    let completed = matches!(manifest.status.as_str(), "done" | "degraded");
    if completed
        || read_utf8_at_most(&dir.join("report.json"), MAX_REPORT_BYTES, "Run report")
            .ok()
            .and_then(|json| serde_json::from_str::<crate::models::PipelineReport>(&json).ok())
            .is_some()
    {
        // The report can be durable while the final manifest never was.
        let interrupted = manifest.status == "running";
        if interrupted {
            manifest.status = "interrupted".to_string();
        }
        if recovered_response_index || interrupted {
            write_manifest(dir, manifest)?;
        }
        return Ok(recovered_response_index);
    }
    // A re-run always starts from the captured document. If it was never
    // written, this job stopped before there was a safe restart point. It
    // still stopped: History must not keep showing it as running.
    if let Err(error) = captured_document_rel_path(dir, manifest) {
        if manifest.status == "running" {
            manifest.status = "interrupted".to_string();
            write_manifest(dir, manifest)?;
        }
        return Err(error);
    }

    let checkpoint_dir = dir.join("artifacts").join("checkpoints");
    let mut checkpoints = Vec::new();
    if let Ok(entries) = fs::read_dir(&checkpoint_dir) {
        let mut walk = crate::safety::WalkBudget::new("Incomplete-run checkpoint recovery");
        for entry in entries.flatten() {
            walk.entry()?;
            let path = entry.path();
            let is_regular = entry
                .file_type()
                .map(|kind| kind.is_file() && !kind.is_symlink())
                .unwrap_or(false);
            if !is_regular || path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;
                if entry
                    .metadata()
                    .map(|metadata| metadata.nlink() > 1)
                    .unwrap_or(true)
                {
                    continue;
                }
            }
            checkpoints.push(path);
        }
    }
    checkpoints.sort();

    let mut outputs = Vec::new();
    let mut checkpoint_failures = Vec::new();
    let mut completed = std::collections::HashSet::<String>::new();
    for path in &checkpoints {
        let Ok(json) = read_utf8_at_most(path, MAX_REPORT_BYTES, "Step checkpoint") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
            continue;
        };
        if value
            .get("checkpointVersion")
            .and_then(serde_json::Value::as_u64)
            == Some(1)
        {
            if let (Ok(steps), Ok(wave)) = (
                serde_json::from_value::<Vec<String>>(value["steps"].clone()),
                serde_json::from_value::<Vec<crate::models::StepOutput>>(value["outputs"].clone()),
            ) {
                outputs.retain(|o: &crate::models::StepOutput| {
                    !steps
                        .iter()
                        .any(|s| o.step_id.split('/').next() == Some(s.as_str()))
                });
                completed.extend(steps);
                outputs.extend(wave);
            }
        } else if let Some(failure) = value
            .get("failure")
            .cloned()
            .and_then(|failure| serde_json::from_value(failure).ok())
        {
            checkpoint_failures.push(failure);
        } else if let Ok(output) = serde_json::from_value::<crate::models::StepOutput>(value) {
            outputs.push(output);
        }
    }

    // Legacy and provisional checkpoints are valuable evidence, but cannot
    // prove a logical step completed. Rerun that entire step conservatively.
    let incomplete: std::collections::BTreeSet<_> = outputs
        .iter()
        .map(|o| {
            o.step_id
                .split('/')
                .next()
                .unwrap_or(&o.step_id)
                .to_string()
        })
        .filter(|id| !completed.contains(id))
        .collect();
    for id in incomplete {
        checkpoint_failures.push(crate::models::StepFailure {
            step_label: id.clone(),
            step_id: id,
            phase: "recovery".into(),
            error: "Interrupted before logical-step completion was durably recorded".into(),
        });
    }

    let orientation = read_utf8_at_most(
        &dir.join("context").join("orientation.json"),
        MAX_REPORT_BYTES,
        "Orientation map",
    )
    .ok()
    .and_then(|json| serde_json::from_str(&json).ok())
    .unwrap_or(serde_json::Value::Null);
    let failed_steps = if checkpoint_failures.is_empty() {
        recovery_failure(manifest)
    } else if manifest.status == "cancelled" {
        // Preserve why the run stopped alongside incomplete logical-step IDs.
        let mut failures = recovery_failure(manifest);
        failures.extend(checkpoint_failures);
        failures
    } else {
        checkpoint_failures
    };
    let mut report = crate::models::PipelineReport {
        orientation,
        step_outputs: outputs,
        failed_steps,
        products: Default::default(),
        quality: crate::models::ReportQuality {
            schema_version: 1,
            status: "partial".to_string(),
            limitations: vec![
                "This report was reconstructed from durable checkpoints after an interrupted run."
                    .to_string(),
            ],
            ..Default::default()
        },
        referee_reports: Vec::new(),
        editor: None,
        report_date: chrono::DateTime::parse_from_rfc3339(&manifest.created)
            .map(|date| date.date_naive())
            .unwrap_or_else(|_| chrono::Local::now().date_naive()),
        paper_hash: manifest
            .run_id
            .split('_')
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    if let Some(config) = captured_workflow_config(dir) {
        report.products = crate::findings::build_run_products(&config, &report.step_outputs);
    } else {
        crate::findings::ensure_legacy_products(&mut report);
    }
    let report_json = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("Failed to serialize recovered report: {error}"))?;
    write_text_atomic(dir, "report.json", &report_json)?;
    let settings = crate::settings::load();
    let markdown = crate::output::render_markdown(
        &report,
        None,
        std::time::Duration::from_secs(manifest.duration_secs),
        &settings,
    );
    write_text_atomic(dir, "report.md", markdown.as_bytes())?;

    register_recovered_artifact(manifest, dir, "report.md", "Recovered report", "report");
    register_recovered_artifact(
        manifest,
        dir,
        "report.json",
        "Recovered report data",
        "context",
    );
    register_recovered_artifact(
        manifest,
        dir,
        "context/document_bundle.json",
        "Document bundle",
        "document",
    );
    register_recovered_artifact(
        manifest,
        dir,
        "context/document.md",
        "Readable document",
        "document",
    );
    register_recovered_artifact(
        manifest,
        dir,
        "context/blocks.jsonl",
        "Document blocks",
        "document",
    );
    register_recovered_directory(manifest, dir, "artifacts/pages", "pages");
    register_recovered_directory(manifest, dir, "artifacts/figures", "figures");
    register_recovered_directory(manifest, dir, "artifacts/document/figures", "figures");
    register_recovered_directory(manifest, dir, "artifacts/agent-responses", "agent_response");
    register_recovered_artifact(
        manifest,
        dir,
        "context/orientation.json",
        "Orientation map",
        "context",
    );
    register_recovered_artifact(
        manifest,
        dir,
        "context/workflow.json",
        "Workflow snapshot",
        "context",
    );
    for path in checkpoints {
        let Ok(relative) = path.strip_prefix(dir) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        let label = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Step checkpoint");
        register_recovered_artifact(manifest, dir, &relative, label, "checkpoint");
    }

    if manifest.status == "running" {
        manifest.status = "interrupted".to_string();
    }
    manifest.step_count = report.step_outputs.len() as u32;
    for failure in &report.failed_steps {
        if !manifest
            .failed_steps
            .iter()
            .any(|label| label == &failure.step_label)
        {
            manifest.failed_steps.push(failure.step_label.clone());
        }
    }
    write_manifest(dir, manifest)?;
    Ok(true)
}

/// Recover one run that has already stopped, so it can be re-run while other
/// reviews are running. The store-wide pass below must wait for them, because
/// it cannot tell their runs from a dead one; here the manifest can. Only a
/// `running` run can still have a writer, and that status is left alone. The
/// caller holds a run slot, which keeps other Pipeline processes out.
pub fn recover_stopped_run(run_id: &str) -> Result<bool, String> {
    validate_run_id(run_id)?;
    recover_stopped_run_dir(&runs_dir()?.join(run_id))
}

pub(super) fn recover_stopped_run_dir(dir: &Path) -> Result<bool, String> {
    let json = read_utf8_at_most(
        &dir.join("manifest.json"),
        MAX_MANIFEST_BYTES,
        "Run manifest",
    )?;
    let mut manifest = serde_json::from_str::<RunManifest>(&json)
        .map_err(|error| format!("Invalid run manifest: {error}"))?;
    if !resumable_status(&manifest.status) {
        return Ok(false);
    }
    recover_resumable_run_dir(dir, &mut manifest)
}

/// Recover runs stopped by an abort, cancellation, provider failure, or a
/// report-persistence failure. The global run lock is acquired first, so a
/// second Pipeline process can never rewrite a genuinely active run.
/// Completed step checkpoints become a partial `report.json`, allowing the
/// normal re-run path to reuse them.
pub fn recover_resumable_runs() -> Result<usize, String> {
    // This process may itself hold the guard (the OS lock is per file, not per
    // process); never rewrite the run it is executing.
    if crate::commands::pipeline_is_running() {
        return Ok(0);
    }
    recover_resumable_runs_at(&runs_dir()?, &run_lock_path()?)
}

/// Recovery over an explicit runs directory and lock file.
pub(super) fn recover_resumable_runs_at(root: &Path, lock_path: &Path) -> Result<usize, String> {
    use fs2::FileExt as _;
    let lock = open_run_lock(lock_path)
        .map_err(|error| format!("Failed to open Pipeline recovery lock: {error}"))?;
    if lock.try_lock_exclusive().is_err() {
        return Ok(0);
    }

    let recovered = recover_runs_in(root, store_walk());
    let _ = fs2::FileExt::unlock(&lock);
    recovered
}

/// The walk over the run store. Its entries are app-owned and it must reach
/// every run however long the store has grown, so only the entry limits apply;
/// the work done for one run is bounded separately.
pub(super) fn store_walk() -> crate::safety::WalkBudget {
    crate::safety::WalkBudget::new("Incomplete-run recovery").without_time_limit()
}

pub(super) fn recover_runs_in(
    root: &Path,
    mut walk: crate::safety::WalkBudget,
) -> Result<usize, String> {
    let mut recovered = 0usize;
    for entry in fs::read_dir(root)
        .map_err(|error| format!("Failed to list runs for recovery: {error}"))?
        .flatten()
    {
        walk.entry()?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        // Startup runs under the exclusive run lock: the safe point to finish
        // deletions that failed partway (see `delete_run`).
        if entry.path().join(DELETE_TOMBSTONE).exists() {
            let _ = fs::remove_dir_all(entry.path());
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        let Ok(json) = read_utf8_at_most(&manifest_path, MAX_MANIFEST_BYTES, "Run manifest") else {
            // Under the exclusive lock a missing manifest cannot be a writer's
            // transient replace window, so this is the safe place to make
            // manifest-less directories visible again (list_runs deliberately
            // skips them to avoid destructive races).
            if !manifest_path.exists() {
                let _ = recover_orphan_manifest(&entry.path());
            }
            continue;
        };
        let Ok(mut manifest) = serde_json::from_str::<RunManifest>(&json) else {
            continue;
        };
        if recover_resumable_run_dir(&entry.path(), &mut manifest).unwrap_or(false) {
            recovered += 1;
        }
    }
    Ok(recovered)
}

/// Make a manifest-less run visible and eligible for normal retention.
/// The artifacts are left untouched; users can inspect the directory externally
/// or delete it from history.
pub(super) fn recover_orphan_manifest(dir: &Path) -> Option<RunManifest> {
    // A tombstone marks a deletion that failed partway. Finish deleting the
    // remnant instead of resurrecting it as a synthetic failed run.
    if dir.join(DELETE_TOMBSTONE).exists() {
        let _ = fs::remove_dir_all(dir);
        return None;
    }
    let run_id = dir.file_name()?.to_str()?.to_string();
    validate_run_id(&run_id).ok()?;
    let created = dir
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(chrono::DateTime::<chrono::Local>::from)
        .unwrap_or_else(chrono::Local::now)
        .to_rfc3339();
    // A current run always captures only document.md; legacy writers always
    // captured extracted_text.md first. This narrow inference keeps current
    // crash recovery working when manifest.json itself is lost or corrupt.
    let artifact_schema_version = if crate::safety::open_regular_file(&dir.join(DOCUMENT_TEXT_PATH))
        .is_ok()
        && crate::safety::open_regular_file(&dir.join(LEGACY_EXTRACTED_TEXT_PATH)).is_err()
    {
        CURRENT_ARTIFACT_SCHEMA_VERSION
    } else {
        0
    };
    let mut manifest = RunManifest {
        artifact_schema_version,
        run_id,
        created,
        input_path: String::new(),
        input_mode: String::new(),
        input_interpretation: String::new(),
        input_identity: Default::default(),
        profile_id: String::new(),
        profile_name: String::new(),
        workflow_source: String::new(),
        workflow_fingerprint: String::new(),
        specialist_catalog_revision: String::new(),
        provider: String::new(),
        artifacts: Vec::new(),
        page_artifacts: None,
        status: "failed".to_string(),
        duration_secs: 0,
        usage: Default::default(),
        step_count: 0,
        failed_steps: vec!["Run ended before manifest finalization".to_string()],
        title: String::new(),
        tags: Vec::new(),
        variables: Default::default(),
        extra_inputs: Default::default(),
        extra_input_sources: Default::default(),
        parent_run_id: None,
    };
    register_recovered_artifact(
        &mut manifest,
        dir,
        "context/workflow.json",
        "Workflow snapshot",
        "context",
    );
    register_recovered_directory(
        &mut manifest,
        dir,
        "artifacts/agent-responses",
        "agent_response",
    );
    write_manifest(dir, &manifest).ok()?;
    Some(manifest)
}

/// True when a manifest that failed `RunManifest` deserialization should be
/// left on disk untouched rather than quarantined: the file exists and its
/// bytes are still valid JSON, which points to a version skew (foreign but
/// intact) rather than corruption. Preserving it prevents a downgrade or
/// side-by-side install from silently rewriting healthy runs as "failed".
pub(super) fn manifest_is_foreign_but_valid(content: Option<&str>, manifest_exists: bool) -> bool {
    manifest_exists
        && content.is_some_and(|content| serde_json::from_str::<serde_json::Value>(content).is_ok())
}

pub(super) fn recover_broken_manifest(dir: &Path, existed: bool) -> Option<RunManifest> {
    if existed {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let source = dir.join("manifest.json");
        let backup = dir.join(format!("manifest.corrupt-{stamp}.json"));
        // Preserve the bytes for support/recovery. If the rename loses a race
        // with a writer, leave the new manifest alone and try again next list.
        if fs::rename(source, backup).is_err() {
            return None;
        }
    }
    recover_orphan_manifest(dir)
}
