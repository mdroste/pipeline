use super::*;

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
    let report_is_valid =
        read_utf8_at_most(&dir.join("report.json"), MAX_REPORT_BYTES, "Run report")
            .ok()
            .and_then(|json| serde_json::from_str::<crate::models::PipelineReport>(&json).ok())
            .is_some();
    if report_is_valid {
        if recovered_response_index {
            write_manifest(dir, manifest)?;
        }
        return Ok(recovered_response_index);
    }
    // A completed run deleted its step checkpoints at finalization, so the
    // rebuild below could only produce an empty report — and would replace a
    // possibly-good report.md along with the unreadable report.json (for
    // example one over the read cap, or damaged by an external tool). Leave
    // completed runs untouched rather than destroying delivered output.
    if matches!(manifest.status.as_str(), "done" | "degraded") {
        if recovered_response_index {
            write_manifest(dir, manifest)?;
        }
        return Ok(recovered_response_index);
    }
    // A re-run always starts from the captured document. If it was never
    // written, this job stopped before there was a safe restart point.
    captured_document_rel_path(dir, manifest)?;

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
    for path in &checkpoints {
        let Ok(json) = read_utf8_at_most(path, MAX_REPORT_BYTES, "Step checkpoint") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
            continue;
        };
        if let Some(failure) = value
            .get("failure")
            .cloned()
            .and_then(|failure| serde_json::from_value(failure).ok())
        {
            checkpoint_failures.push(failure);
        } else if let Ok(output) = serde_json::from_value::<crate::models::StepOutput>(value) {
            outputs.push(output);
        }
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

/// Recover runs stopped by an abort, cancellation, provider failure, or a
/// report-persistence failure. The global run lock is acquired first, so a
/// second Pipeline process can never rewrite a genuinely active run.
/// Completed step checkpoints become a partial `report.json`, allowing the
/// normal re-run path to reuse them.
pub fn recover_resumable_runs() -> Result<usize, String> {
    use fs2::FileExt as _;
    let root = runs_dir()?;
    let pipeline_dir = root
        .parent()
        .ok_or("Cannot resolve Pipeline data directory")?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(pipeline_dir.join("run.lock"))
        .map_err(|error| format!("Failed to open Pipeline recovery lock: {error}"))?;
    if lock.try_lock_exclusive().is_err() {
        return Ok(0);
    }

    let mut recovered = 0usize;
    let mut walk = crate::safety::WalkBudget::new("Incomplete-run recovery");
    for entry in fs::read_dir(&root)
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
    let _ = fs2::FileExt::unlock(&lock);
    Ok(recovered)
}

/// List every run on disk as a summary row, newest first. Unreadable or
/// malformed manifests are skipped rather than failing the whole listing.
pub fn list_runs() -> Result<Vec<RunSummary>, String> {
    let dir = runs_dir()?;
    let mut summaries = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(summaries);
    };
    let mut walk = crate::safety::WalkBudget::new("Run history listing");
    for entry in entries.flatten() {
        walk.entry()?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        let manifest_too_large = fs::symlink_metadata(&manifest_path)
            .map(|metadata| metadata.len() > MAX_MANIFEST_BYTES as u64)
            .unwrap_or(false);
        let manifest = if manifest_too_large {
            // Never rename and replace an oversized manifest: doing so can
            // destroy a valid run index merely because an older writer
            // exceeded the newer reader's limit.
            None
        } else {
            let Some(content) = read_manifest_content_for_listing(&manifest_path) else {
                // The bytes could not be read at all: NotFound or a sharing
                // violation while a writer replaces the file on Windows, or a
                // directory without a manifest. Quarantining or synthesizing a
                // manifest without having read the bytes destructively rewrites
                // healthy runs; skip the directory for this listing pass.
                continue;
            };
            serde_json::from_str::<RunManifest>(&content)
                .ok()
                .or_else(|| {
                    // A manifest that is valid JSON but does not deserialize as
                    // RunManifest was almost certainly written by a different
                    // app version (e.g. a field type changed). Quarantining it
                    // would convert a healthy run into a synthetic "failed" row
                    // and hide the real data on a downgrade or side-by-side
                    // install. Only recover when the read bytes are not valid
                    // JSON at all.
                    if manifest_is_foreign_but_valid(Some(content.as_str()), true) {
                        None
                    } else {
                        recover_broken_manifest(&entry.path(), true)
                    }
                })
        };
        if let Some(manifest) = manifest {
            let mut summary = manifest.to_summary();
            summary.resumable = run_has_resume_files(&entry.path(), &manifest);
            summaries.push(summary);
        }
    }
    // Sort by created timestamp (RFC3339 sorts lexically), newest first.
    summaries.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(summaries)
}

/// The manifest bytes when they could actually be read, or `None` on any
/// open/read failure. Windows writers replace `manifest.json` in place, so
/// NotFound / permission / sharing-violation errors here are routinely
/// transient; only content that was fully read may be judged — and possibly
/// quarantined — by the caller. Invalid UTF-8 is read lossily so genuinely
/// corrupt bytes still fail JSON parsing rather than masquerading as an
/// unreadable file.
fn read_manifest_content_for_listing(path: &Path) -> Option<String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Locate the newest completed report in the same revision lineage. Stable
/// input path is the primary lineage key; content hash also finds identical
/// copies moved back to the same logical input.
pub fn load_latest_report_for_input(
    input_path: &str,
    paper_hash: &str,
) -> Result<Option<crate::models::PipelineReport>, String> {
    let normalized_input = Path::new(input_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(input_path));
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    let mut walk = crate::safety::WalkBudget::new_cancellable("Prior report discovery");
    for entry in fs::read_dir(runs_dir()?).map_err(|e| format!("Failed to list runs: {e}"))? {
        walk.entry()?;
        let Ok(entry) = entry else { continue };
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let manifest = read_utf8_at_most(
            &entry.path().join("manifest.json"),
            MAX_MANIFEST_BYTES,
            "Run manifest",
        )
        .ok()
        .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok());
        let Some(manifest) = manifest else { continue };
        if manifest.status == "running"
            || manifest.status == "failed"
            || manifest.status == "cancelled"
            || manifest.status == "interrupted"
        {
            continue;
        }
        let same_path = Path::new(&manifest.input_path)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(&manifest.input_path))
            == normalized_input;
        let same_hash = manifest.run_id.starts_with(&format!("{paper_hash}_"));
        if same_path || same_hash {
            candidates.push((manifest.created, entry.path().join("report.json")));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in candidates {
        let Ok(json) = read_utf8_at_most(&path, MAX_REPORT_BYTES, "Run report") else {
            continue;
        };
        if let Ok(report) = serde_json::from_str(&json) {
            return Ok(Some(report));
        }
    }
    Ok(None)
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

fn recover_broken_manifest(dir: &Path, existed: bool) -> Option<RunManifest> {
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

/// Update a run's user-assigned title and tags in place. Tags are trimmed and
/// de-duplicated; empties are dropped.
pub fn update_run_meta(run_id: &str, title: &str, tags: &[String]) -> Result<(), String> {
    validate_run_id(run_id)?;
    let mut manifest = load_manifest(run_id)?;
    if manifest.status == "running" {
        return Err("A running job cannot be renamed or retagged".to_string());
    }
    let title = title.trim();
    if title.len() > 500 {
        return Err("Run title cannot exceed 500 bytes".to_string());
    }
    if tags.len() > 50 || tags.iter().any(|tag| tag.trim().len() > 100) {
        return Err("Runs support at most 50 tags of 100 bytes each".to_string());
    }
    manifest.title = title.to_string();
    let mut seen = std::collections::HashSet::new();
    manifest.tags = tags
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && seen.insert(t.clone()))
        .collect();
    let dir = runs_dir()?.join(run_id);
    write_manifest(&dir, &manifest)
}

#[derive(Debug, Clone, Serialize)]
pub struct TrashedRun {
    pub run_id: String,
    pub deleted_at: String,
    pub title: String,
    pub input_name: String,
}

fn trash_runs_dir() -> Result<PathBuf, String> {
    let runs = runs_dir()?;
    let pipeline = runs
        .parent()
        .ok_or_else(|| "Cannot resolve Pipeline storage directory".to_string())?;
    let trash = pipeline.join("trash").join("runs");
    fs::create_dir_all(&trash).map_err(|error| format!("Cannot create run Trash: {error}"))?;
    Ok(trash)
}

/// Move a completed run into recoverable Trash. The move is atomic because
/// Trash lives beside the run store on the same filesystem.
pub fn delete_run(run_id: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    if runs_dir()?.join(run_id).join(".task-pin").exists() { return Err("This run is retained by a task that has not yet adopted its result".into()); }
    let manifest = load_manifest(run_id)?;
    if manifest.status == "running" {
        return Err("A running job cannot be deleted".to_string());
    }
    let base = runs_dir()?
        .canonicalize()
        .map_err(|e| format!("Cannot resolve runs dir: {e}"))?;
    let dir = base.join(run_id);
    let canonical = dir
        .canonicalize()
        .map_err(|_| "Run not found".to_string())?;
    if !canonical.starts_with(&base) || canonical == base {
        return Err("Invalid run id".into());
    }
    let trash = trash_runs_dir()?;
    let destination = trash.join(run_id);
    if destination.exists() {
        return Err(
            "A trashed copy of this run already exists; restore or permanently delete it first"
                .to_string(),
        );
    }
    fs::rename(&canonical, &destination)
        .map_err(|error| format!("Failed to move run to Trash: {error}"))?;
    sync_directory(&base)?;
    sync_directory(&trash)?;
    Ok(())
}

pub fn list_trashed_runs() -> Result<Vec<TrashedRun>, String> {
    let trash = trash_runs_dir()?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&trash)
        .map_err(|error| format!("Cannot list run Trash: {error}"))?
        .flatten()
    {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let Some(run_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if validate_run_id(&run_id).is_err() {
            continue;
        }
        let deleted_at = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map(chrono::DateTime::<chrono::Local>::from)
            .map(|time| time.to_rfc3339())
            .unwrap_or_default();
        let manifest = read_utf8_at_most(
            &entry.path().join("manifest.json"),
            MAX_MANIFEST_BYTES,
            "Trashed run manifest",
        )
        .ok()
        .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok());
        let title = manifest
            .as_ref()
            .map(|value| value.title.clone())
            .unwrap_or_default();
        let input_name = manifest
            .as_ref()
            .and_then(|value| Path::new(&value.input_path).file_name())
            .and_then(|value| value.to_str())
            .unwrap_or("Report")
            .to_string();
        entries.push(TrashedRun {
            run_id,
            deleted_at,
            title,
            input_name,
        });
    }
    entries.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    Ok(entries)
}

pub fn restore_trashed_run(run_id: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    let trash = trash_runs_dir()?;
    let source = trash.join(run_id);
    let canonical = source
        .canonicalize()
        .map_err(|_| "Trashed run not found".to_string())?;
    if canonical.parent() != Some(trash.as_path()) || !canonical.is_dir() {
        return Err("Invalid trashed run id".to_string());
    }
    let root = runs_dir()?;
    let destination = root.join(run_id);
    if destination.exists() {
        return Err("A run with this id already exists in History".to_string());
    }
    crate::safety::open_regular_file(&canonical.join("manifest.json"))
        .map_err(|_| "Trashed run has no valid manifest".to_string())?;
    fs::rename(&canonical, &destination)
        .map_err(|error| format!("Failed to restore run from Trash: {error}"))?;
    sync_directory(&root)?;
    sync_directory(&trash)
}

/// Deliberate, irreversible deletion of a run that is already in Trash.
pub fn permanently_delete_trashed_run(run_id: &str) -> Result<(), String> {
    validate_run_id(run_id)?;
    let trash = trash_runs_dir()?
        .canonicalize()
        .map_err(|error| format!("Cannot resolve run Trash: {error}"))?;
    let source = trash.join(run_id);
    let canonical = source
        .canonicalize()
        .map_err(|_| "Trashed run not found".to_string())?;
    if canonical.parent() != Some(trash.as_path()) {
        return Err("Invalid trashed run id".to_string());
    }
    fs::remove_dir_all(&canonical)
        .map_err(|error| format!("Failed to permanently delete trashed run: {error}"))?;
    sync_directory(&trash)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("Failed to sync '{}': {error}", path.display()))?;
    Ok(())
}
