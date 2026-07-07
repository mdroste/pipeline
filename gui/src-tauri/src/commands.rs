//! Tauri command handlers.

use crate::models::PipelineReport;
use crate::models::ReportSummary;
use crate::pipeline::{executor, extract, orient, reconcile};
use crate::pipeline_config::{self, PipelineConfig, ProfileSummary};
use crate::output;
use crate::storage;
use std::io::Write;
use tauri::AppHandle;

/// Maximum file size for imported configs (10 MB). Pipeline configs are
/// small JSON; anything larger is almost certainly the wrong file.
const MAX_IMPORT_SIZE: u64 = 10_000_000;

/// Read a file for import, rejecting files above the size limit.
fn read_import_file(path: &str) -> Result<String, String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("Failed to read {path}: {e}"))?;
    if meta.len() > MAX_IMPORT_SIZE {
        return Err(format!(
            "File is too large ({:.1} MB). Import files should be under {} MB.",
            meta.len() as f64 / 1_000_000.0,
            MAX_IMPORT_SIZE / 1_000_000,
        ));
    }
    std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read: {e}"))
}

// --- Pipeline ---

static CANCEL_FLAG: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Guard: true while a pipeline is running. Prevents concurrent runs.
static PIPELINE_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// RAII guard that cleans up pipeline state on drop (including panics).
struct PipelineGuard;

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        kill_all_children();
        PIPELINE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        crate::pipeline::api_common::set_allowed_dirs(vec![]);
        crate::pipeline::api_common::set_write_dir(None);
        // Stop mirroring the console to disk (also flushes/closes the file).
        crate::pipeline::logging::set_log_sink(None);
        reset_pass_cancels();
    }
}

/// PIDs of active child processes (claude/gemini/codex subprocesses).
/// Populated by `register_child_pid`, cleared by `unregister_child_pid`.
static CHILD_PIDS: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

/// (pass key, pid) for per-pass cancellation. A pass key is the step key
/// ("technical/claude") the executor tags each `call_llm` with.
static PASS_PIDS: std::sync::Mutex<Vec<(String, u32)>> = std::sync::Mutex::new(Vec::new());
/// Pass keys the user asked to cancel this run. Checked by the executor's retry
/// loop so a cancelled pass fails instead of retrying.
static CANCELLED_PASSES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Whether a specific pass was cancelled by the user.
pub fn is_pass_cancelled(pass_key: &str) -> bool {
    CANCELLED_PASSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|k| k == pass_key)
}

/// Clear per-pass cancellation state (called at run start/end).
fn reset_pass_cancels() {
    CANCELLED_PASSES.lock().unwrap_or_else(|e| e.into_inner()).clear();
    PASS_PIDS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

// ── Batch queue (1.3.1) ─────────────────────────────────────────────
//
// A batch is a list of input paths run one at a time through the normal
// pipeline (parallelism stays *inside* a run). The worker holds the same
// PIPELINE_RUNNING guard as a single run, so the two can never overlap. Job
// state is published to the frontend via `batch:progress` events and pollable
// via `get_batch_status`.

/// One job in a batch. Serialized to the frontend as-is.
#[derive(Clone, serde::Serialize)]
pub struct BatchJob {
    pub path: String,
    pub name: String,
    /// "pending" | "running" | "done" | "failed" | "cancelled".
    pub status: String,
    pub run_id: Option<String>,
    pub error: Option<String>,
    pub duration_secs: u64,
}

static BATCH: std::sync::Mutex<Vec<BatchJob>> = std::sync::Mutex::new(Vec::new());
static BATCH_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ── Watch folder (1.3.4) ────────────────────────────────────────────
//
// A background poll of a folder: files that appear after watching starts are
// run through the pipeline one at a time with the active profile. Dependency-
// free (no file-watcher crate) — a 5s poll is plenty for this use case.

#[derive(Clone, Default, serde::Serialize)]
pub struct WatchStatus {
    pub active: bool,
    pub folder: String,
    /// Files processed since watching started (newest last).
    pub processed: Vec<BatchJob>,
}

static WATCH_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WATCH_STATE: std::sync::Mutex<WatchStatus> = std::sync::Mutex::new(WatchStatus {
    active: false,
    folder: String::new(),
    processed: Vec::new(),
});

fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn emit_batch(app: &crate::emit::EventBus) {
    let jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let _ = app.emit_event("batch:progress", serde_json::to_value(&jobs).unwrap_or_default());
}

pub fn is_cancelled() -> bool {
    CANCEL_FLAG.load(std::sync::atomic::Ordering::Acquire)
}

/// Register a child process PID so it can be killed on cancel. Also attributes
/// the PID to the current pass (if any) for per-pass cancellation.
pub fn register_child_pid(pid: u32) {
    // Callers already skip pid 0 (spawn without a real id), but guard here
    // too so kill_all_children never signals pid 0 / process group 0.
    if pid == 0 {
        return;
    }
    CHILD_PIDS.lock().unwrap_or_else(|e| e.into_inner()).push(pid);
    if let Some(pass) = crate::pipeline::logging::current_pass() {
        PASS_PIDS.lock().unwrap_or_else(|e| e.into_inner()).push((pass, pid));
    }
}

/// Unregister a child process PID after it exits.
pub fn unregister_child_pid(pid: u32) {
    CHILD_PIDS.lock().unwrap_or_else(|e| e.into_inner()).retain(|&p| p != pid);
    PASS_PIDS.lock().unwrap_or_else(|e| e.into_inner()).retain(|(_, p)| *p != pid);
}

/// Cancel one pass: mark it cancelled (so it won't retry) and kill its
/// subprocesses. Other passes in the wave keep running.
#[tauri::command]
pub async fn cancel_pass(pass_key: String) -> Result<(), String> {
    CANCELLED_PASSES.lock().unwrap_or_else(|e| e.into_inner()).push(pass_key.clone());
    let pids: Vec<u32> = PASS_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(k, _)| *k == pass_key)
        .map(|(_, p)| *p)
        .collect();
    for pid in pids {
        kill_process(pid);
    }
    Ok(())
}

/// Kill all registered child processes.
fn kill_all_children() {
    let pids = CHILD_PIDS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for pid in pids {
        kill_process(pid);
    }
}

fn kill_process(pid: u32) {
    if pid == 0 { return; }
    #[cfg(unix)]
    {
        // Validate PID fits in a positive i32 to prevent overflow when negating
        if pid > i32::MAX as u32 { return; }
        let pid_i32 = pid as i32;
        unsafe {
            libc::kill(-pid_i32, libc::SIGTERM);
        }
        unsafe {
            libc::kill(pid_i32, libc::SIGTERM);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(0x08000000)
            .status();
    }
}

/// Write a completed run's artifacts and manifest into the run directory:
/// numbered per-step files, `report.md`, `report.json` (structured, for resume),
/// any model-written files, the console log, and the manifest. Returns the run
/// id on success. Shared by fresh runs and re-runs.
fn finalize_run(
    app: &crate::emit::EventBus,
    mut w: crate::runs::RunWriter,
    report: &PipelineReport,
    markdown: &str,
    meta: crate::runs::RunFinishMeta,
) -> Option<String> {
    let outputs = report.all_outputs();
    for (i, output) in outputs.iter().enumerate() {
        let slug = output
            .step_id
            .replace('/', "_")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect::<String>();
        let header = format!(
            "# {}\n\n**Phase**: {} · **Agent**: {}\n\n---\n\n",
            output.step_label,
            output.phase,
            if output.agent.is_empty() { "default" } else { &output.agent },
        );
        let rel = format!("artifacts/{:02}_{}.md", i + 1, slug);
        if let Err(e) = w.add_text(&rel, &output.step_label, "step", &format!("{}{}", header, output.raw_text)) {
            let _ = app.emit_event("pipeline:log", serde_json::json!({ "line": format!("WARNING: {e}") }));
        }
    }
    if let Err(e) = w.add_text("report.md", "Report", "report", markdown) {
        let _ = app.emit_event("pipeline:log", serde_json::json!({ "line": format!("WARNING: {e}") }));
    }
    // Structured report, so a re-run can reload prior step outputs.
    if let Ok(json) = serde_json::to_string_pretty(report) {
        let _ = w.add_text("report.json", "Report data", "context", &json);
    }
    let extra_files = w.register_unlisted("artifacts", "files");
    if extra_files > 0 {
        let _ = app.emit_event("pipeline:log", serde_json::json!({
            "line": format!("Registered {extra_files} model-written supporting files")
        }));
    }
    // Close the console transcript before registering it so the file is complete.
    crate::pipeline::logging::set_log_sink(None);
    let _ = w.register_existing("logs/run.log", "Console log", "context");

    match w.finish(meta) {
        Ok(manifest) => Some(manifest.run_id),
        Err(e) => {
            let _ = app.emit_event("pipeline:log", serde_json::json!({
                "line": format!("WARNING: could not write run manifest: {e}")
            }));
            None
        }
    }
}

/// Purge old runs beyond the retention cap (0 = keep all), logging how many.
fn enforce_retention(app: &crate::emit::EventBus, keep: usize) {
    if keep == 0 {
        return;
    }
    if let Ok(n) = crate::runs::purge_old_runs(keep) {
        if n > 0 {
            let _ = app.emit_event("pipeline:log", serde_json::json!({
                "line": format!("Removed {n} old run(s) to stay within the {keep}-run limit")
            }));
        }
    }
}

#[tauri::command]
pub async fn run_pipeline(
    app: AppHandle,
    paper_path: String,
    diff: bool,
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
) -> Result<serde_json::Value, String> {
    // Prevent concurrent pipeline runs from corrupting shared state
    if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A pipeline is already running".into());
    }
    // RAII guard ensures cleanup runs even on panic
    let _guard = PipelineGuard;
    let bus = crate::emit::from_app(app);
    run_pipeline_inner(
        &bus,
        &paper_path,
        diff,
        variables.unwrap_or_default(),
        extra_inputs.unwrap_or_default(),
    )
    .await
}

/// Headless entry point for the CLI: run the active profile over one input with
/// the given event sink (no Tauri `AppHandle`). Returns the same JSON as the
/// GUI command. Not a Tauri command — called directly from the CLI binary.
pub async fn run_headless(
    bus: crate::emit::EventBus,
    paper_path: &str,
    variables: std::collections::HashMap<String, String>,
    extra_inputs: std::collections::HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A pipeline is already running".into());
    }
    let _guard = PipelineGuard;
    run_pipeline_inner(&bus, paper_path, false, variables, extra_inputs).await
}

async fn run_pipeline_inner(
    app: &crate::emit::EventBus,
    paper_path: &str,
    diff: bool,
    provided_vars: std::collections::HashMap<String, String>,
    provided_inputs: std::collections::HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::Release);
    reset_pass_cancels();
    crate::pipeline::logging::reset_run_usage();
    let pipeline_start = std::time::Instant::now();

    // Set up allowed directories for direct API file reads
    let paper = std::path::Path::new(paper_path);
    let source_dir = if paper.is_dir() {
        paper_path.to_string()
    } else {
        paper.parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    crate::pipeline::api_common::set_allowed_dirs(vec![source_dir.clone()]);

    // Load profile config first so extraction overrides apply.
    let config = pipeline_config::load();

    // Effective run-time variables: the profile's declared defaults, overlaid
    // with whatever the caller provided. Unknown provided keys are kept (a
    // prompt may reference an ad-hoc var).
    let mut variables: std::collections::HashMap<String, String> = config
        .variables
        .iter()
        .map(|v| (v.key.clone(), v.default.clone()))
        .collect();
    variables.extend(provided_vars);

    // Extract paper text
    app.emit_event("pipeline:stage", serde_json::json!({"stage": "extracting"})).ok();
    app.emit_event("pipeline:preprocess", serde_json::json!({
        "phase": "extract",
        "status": "running",
    })).ok();
    let extract_start = std::time::Instant::now();
    let input_mode = extract::effective_input_mode(&config.extraction.input_mode, paper_path);
    let extraction = match input_mode {
        "folder" => extract::ingest_folder(paper_path)?,
        "none" => extract::ingest_none(),
        _ => extract::extract(app, paper_path, &config.extraction).await?,
    };
    let extract_secs = extract_start.elapsed().as_secs();
    app.emit_event("pipeline:log", serde_json::json!({
        "line": format!("Extracted via {} ({} chars, {}s)", extraction.method, extraction.text.len(), extract_secs)
    })).ok();
    for note in &extraction.quality_notes {
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!("WARNING: {note}")
        })).ok();
    }

    // Persist this run's outputs under ~/.pipeline/runs/{run_id}/.
    // Best-effort throughout: persistence failures are logged, never fatal.
    let run_id = format!(
        "{}_{}",
        extraction.paper_hash,
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let mut run_writer = match crate::runs::RunWriter::create(&run_id) {
        Ok(w) => Some(w),
        Err(e) => {
            let _ = app.emit_event("pipeline:log", serde_json::json!({
                "line": format!("WARNING: could not create run directory: {e}")
            }));
            None
        }
    };
    if let Some(w) = run_writer.as_mut() {
        if let Err(e) = w.add_text(
            "context/extracted_text.md",
            "Extracted text",
            "context",
            &extraction.text,
        ) {
            let _ = app.emit_event("pipeline:log", serde_json::json!({
                "line": format!("WARNING: {e}")
            }));
        }
    }

    // Mirror the console to a transcript inside the run directory. Registered
    // in the manifest at finish, after the sink is closed. Best-effort.
    if let Some(w) = run_writer.as_ref() {
        let logs_dir = w.dir().join("logs");
        match std::fs::create_dir_all(&logs_dir)
            .and_then(|_| std::fs::File::create(logs_dir.join("run.log")))
        {
            Ok(f) => crate::pipeline::logging::set_log_sink(Some(f)),
            Err(e) => {
                let _ = app.emit_event("pipeline:log", serde_json::json!({
                    "line": format!("WARNING: could not open run log file: {e}")
                }));
            }
        }
    }

    // Artifact write sandbox: LLM steps may write files into this run's
    // artifacts/ directory and nowhere else. Created up front so every
    // enforcement layer (Write tool validation, CLI permission rules,
    // codex/gemini workspaces) can resolve it. When the run dir couldn't
    // be created, steps fall back to stdout output.
    let artifact_write_dir: Option<String> = run_writer.as_ref().and_then(|w| {
        let dir = w.dir().join("artifacts");
        match std::fs::create_dir_all(&dir) {
            Ok(()) => Some(dir.to_string_lossy().replace('\\', "/")),
            Err(e) => {
                let _ = app.emit_event("pipeline:log", serde_json::json!({
                    "line": format!("WARNING: could not create artifact dir, steps will use stdout output: {e}")
                }));
                None
            }
        }
    });
    crate::pipeline::api_common::set_write_dir(
        artifact_write_dir.as_ref().map(std::path::PathBuf::from),
    );

    // Render page images for PDF inputs so the artifact explorer can show
    // them. Deliberately independent of the extraction method; best-effort.
    if input_mode == "document" && extraction.source_path.to_lowercase().ends_with(".pdf") {
        if let Some(w) = run_writer.as_mut() {
            let pdf = std::path::PathBuf::from(&extraction.source_path);
            let out_dir = w.dir().join("artifacts").join("pages");
            let rendered = tokio::task::spawn_blocking(move || {
                crate::pipeline::extract::render_pdf_pages(&pdf, &out_dir, 300)
            })
            .await;
            match rendered {
                Ok(Ok(names)) => {
                    let count = names.len();
                    for name in &names {
                        let page_num = name
                            .trim_end_matches(".png")
                            .rsplit('-')
                            .next()
                            .and_then(|n| n.parse::<u32>().ok());
                        let label = match page_num {
                            Some(n) => format!("Page {n}"),
                            None => name.clone(),
                        };
                        if let Err(e) =
                            w.register_existing(&format!("artifacts/pages/{name}"), &label, "pages")
                        {
                            let _ = app.emit_event("pipeline:log", serde_json::json!({
                                "line": format!("WARNING: {e}")
                            }));
                        }
                    }
                    let _ = app.emit_event("pipeline:log", serde_json::json!({
                        "line": format!("Rendered {count} page images into the run artifacts")
                    }));
                }
                Ok(Err(e)) => {
                    let _ = app.emit_event("pipeline:log", serde_json::json!({
                        "line": format!("WARNING: page image rendering skipped: {e}")
                    }));
                }
                Err(e) => {
                    let _ = app.emit_event("pipeline:log", serde_json::json!({
                        "line": format!("WARNING: page image rendering task failed: {e}")
                    }));
                }
            }
        }
    }

    // When marker did the extraction, collect the figure images it emitted
    // into the run artifacts. Best-effort.
    if extraction.method == "marker" {
        if let Some(w) = run_writer.as_mut() {
            let images = crate::pipeline::extract::marker_image_files(&extraction.paper_hash);
            if !images.is_empty() {
                let figures_dir = w.dir().join("artifacts").join("figures");
                if let Err(e) = std::fs::create_dir_all(&figures_dir) {
                    let _ = app.emit_event("pipeline:log", serde_json::json!({
                        "line": format!("WARNING: could not create figures dir: {e}")
                    }));
                } else {
                    let mut copied = 0usize;
                    for src in &images {
                        let Some(name) = src.file_name().and_then(|n| n.to_str()) else { continue };
                        if std::fs::copy(src, figures_dir.join(name)).is_ok()
                            && w.register_existing(&format!("artifacts/figures/{name}"), name, "figures").is_ok()
                        {
                            copied += 1;
                        }
                    }
                    let _ = app.emit_event("pipeline:log", serde_json::json!({
                        "line": format!("Collected {copied} figure images from marker output")
                    }));
                }
            }
        }
    }
    if is_cancelled() { return Err("Pipeline cancelled".into()); }

    // Cache the extracted text by paper hash so users can inspect it after the run.
    // Temp files vanish when the process exits; the cache persists until deleted.
    let cached_paper_path = match cache_paper_text(&extraction.paper_hash, &extraction.text) {
        Ok(p) => Some(p),
        Err(e) => {
            // Caching is best-effort; a failure here shouldn't stop the run.
            let _ = app.emit_event("pipeline:log", serde_json::json!({
                "line": format!("WARNING: failed to cache extracted text: {e}")
            }));
            None
        }
    };

    let mut tmp = tempfile::Builder::new()
        .prefix("pipeline_paper_")
        .suffix(".txt")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    tmp.write_all(extraction.text.as_bytes())
        .map_err(|e| format!("Failed to write temp file: {e}"))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {e}"))?;
    let paper_text_path = tmp.path().to_string_lossy().replace('\\', "/");

    app.emit_event("pipeline:preprocess", serde_json::json!({
        "phase": "extract",
        "status": "done",
        "method": extraction.method,
        "chars": extraction.text.len(),
        "elapsed_secs": extract_secs,
        "paper_hash": extraction.paper_hash,
        "cached_path": cached_paper_path,
    })).ok();

    // Build orientation map (optional)
    let mut _orient_tmp = None; // hold tempfile alive
    let orientation;
    let orientation_path;

    if config.use_orientation {
        app.emit_event("pipeline:stage", serde_json::json!({"stage": "orienting"})).ok();
        app.emit_event("pipeline:preprocess", serde_json::json!({
            "phase": "orient",
            "status": "running",
        })).ok();
        let orient_start = std::time::Instant::now();
        let survey_template =
            orient::resolve_survey_template(&config.orientation_prompt, input_mode);
        orientation =
            orient::build_orientation_map(app, &extraction, survey_template.as_deref()).await?;
        let orient_secs = orient_start.elapsed().as_secs();
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!("Orientation map built ({}s)", orient_secs)
        })).ok();
        if is_cancelled() { return Err("Pipeline cancelled".into()); }

        let orientation_json = serde_json::to_string(&orientation)
            .map_err(|e| format!("Failed to serialize orientation map: {e}"))?;
        let mut orient_file = tempfile::Builder::new()
            .prefix("pipeline_orient_")
            .suffix(".json")
            .tempfile()
            .map_err(|e| format!("Failed to create orientation temp file: {e}"))?;
        orient_file.write_all(orientation_json.as_bytes())
            .map_err(|e| format!("Failed to write orientation temp file: {e}"))?;
        orient_file.flush()
            .map_err(|e| format!("Failed to flush orientation temp file: {e}"))?;
        orientation_path = orient_file.path().to_string_lossy().replace('\\', "/");
        if let Some(w) = run_writer.as_mut() {
            if let Err(e) = w.add_text(
                "context/orientation.json",
                "Orientation map",
                "context",
                &orientation_json,
            ) {
                let _ = app.emit_event("pipeline:log", serde_json::json!({
                    "line": format!("WARNING: {e}")
                }));
            }
        }
        let orient_bytes = orientation_json.len();
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!("Orientation map written to temp file ({orient_bytes} bytes)")
        })).ok();
        app.emit_event("pipeline:preprocess", serde_json::json!({
            "phase": "orient",
            "status": "done",
            "bytes": orient_bytes,
            "elapsed_secs": orient_secs,
        })).ok();
        _orient_tmp = Some(orient_file);
    } else {
        app.emit_event("pipeline:log", serde_json::json!({
            "line": "Orientation map disabled for this profile"
        })).ok();
        app.emit_event("pipeline:preprocess", serde_json::json!({
            "phase": "orient",
            "status": "skipped",
        })).ok();
        orientation = serde_json::to_value(crate::models::OrientationMap::empty(&extraction.text))
            .map_err(|e| format!("Failed to build orientation placeholder: {e}"))?;
        orientation_path = String::new();
    }

    // {paper_type} resolves only for paper-shaped surveys; empty otherwise.
    let paper_type = crate::models::paper_view(&orientation)
        .map(|v| v.metadata.paper_type.to_string())
        .unwrap_or_default();
    let survey_hint = crate::models::survey_hint(&orientation);

    // Extra named inputs (1.2.4): extract each declared slot's file through the
    // same cascade, write the text to a temp file, and expose its path to
    // prompts as {input:key}. Temp files are held alive until the run finishes.
    let mut resolved_inputs: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut extra_tmps: Vec<tempfile::NamedTempFile> = Vec::new();
    let mut extra_dirs: Vec<String> = Vec::new();
    for slot in &config.extraction.extra_inputs {
        let provided = provided_inputs.get(&slot.key).map(|s| s.trim()).filter(|s| !s.is_empty());
        let path = match provided {
            Some(p) => p,
            None => {
                if slot.required {
                    let name = if slot.label.is_empty() { &slot.key } else { &slot.label };
                    return Err(format!("Missing required input '{name}'"));
                }
                continue;
            }
        };
        if let Some(dir) = std::path::Path::new(path).parent() {
            extra_dirs.push(dir.to_string_lossy().to_string());
        }
        let ex = match slot.mode.as_str() {
            "folder" => extract::ingest_folder(path)?,
            _ => extract::extract(app, path, &config.extraction).await?,
        };
        let mut tf = tempfile::Builder::new()
            .prefix("pipeline_input_")
            .suffix(".txt")
            .tempfile()
            .map_err(|e| format!("Failed to create temp file for input '{}': {e}", slot.key))?;
        tf.write_all(ex.text.as_bytes())
            .map_err(|e| format!("Failed to write input '{}': {e}", slot.key))?;
        tf.flush().ok();
        let p = tf.path().to_string_lossy().replace('\\', "/");
        resolved_inputs.insert(slot.key.clone(), p);
        extra_tmps.push(tf);
        app.emit_event("pipeline:log", serde_json::json!({
            "line": format!("Extra input '{}' extracted via {} ({} chars)", slot.key, ex.method, ex.text.len())
        })).ok();
    }
    // Widen the read sandbox to include the extra inputs' directories.
    if !extra_dirs.is_empty() {
        let mut dirs = vec![source_dir.clone()];
        dirs.extend(extra_dirs);
        crate::pipeline::api_common::set_allowed_dirs(dirs);
    }
    if is_cancelled() { return Err("Pipeline cancelled".into()); }

    let result = executor::execute_steps(
        app,
        &config,
        &orientation_path,
        &orientation,
        &paper_text_path,
        &extraction.source_path,
        &paper_type,
        &survey_hint,
        &variables,
        &resolved_inputs,
        &std::collections::HashMap::new(), // no preloaded steps for a fresh run
        artifact_write_dir.as_deref(),
    )
    .await?;
    drop(extra_tmps); // keep temp files alive until steps have run

    // Steps are done — close the write window before rendering/reconciling.
    crate::pipeline::api_common::set_write_dir(None);

    if is_cancelled() { return Err("Pipeline cancelled".into()); }

    let report = PipelineReport {
        orientation,
        step_outputs: result.outputs,
        failed_steps: result.failed_steps,
        referee_reports: vec![],
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: extraction.paper_hash.clone(),
    };

    // Optional diff
    let mut diff_text = None;
    if diff {
        if let Ok(Some(prior)) = storage::load_latest_report(&extraction.paper_hash) {
            if let Ok(dt) = reconcile::reconcile(app, &prior, &report).await {
                diff_text = Some(dt);
            }
        }
    }

    if let Err(e) = storage::save_report(&report) {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!("WARNING: Failed to save report to history: {e}. The report is still available but won't appear in history or be available for diffing.") }),
        );
    }
    let elapsed = pipeline_start.elapsed();
    let settings = crate::settings::load();
    let markdown = output::render_markdown(&report, diff_text.as_deref(), elapsed, &settings);

    // Finish the run directory: per-step artifacts, report, manifest.
    let profile_name = pipeline_config::load_profile(&settings.active_profile)
        .map(|p| p.name)
        .unwrap_or_default();
    let status = if report.failed_steps.is_empty() { "done" } else { "partial" };
    let meta = crate::runs::RunFinishMeta {
        input_path: paper_path.to_string(),
        input_mode: input_mode.to_string(),
        profile_id: settings.active_profile.clone(),
        profile_name,
        provider: settings.preferred_provider.clone(),
        status: status.to_string(),
        duration_secs: elapsed.as_secs(),
        usage: crate::pipeline::logging::run_usage(),
        step_count: report.all_outputs().len() as u32,
        failed_steps: report.failed_steps.iter().map(|f| f.step_label.clone()).collect(),
        variables: variables.clone(),
        parent_run_id: None,
    };
    let mut finished_run_id: Option<String> = None;
    if let Some(w) = run_writer.take() {
        finished_run_id = finalize_run(app, w, &report, &markdown, meta);
    }
    enforce_retention(app, settings.max_saved_runs as usize);

    app.emit_event("pipeline:stage", serde_json::json!({"stage": "done"})).ok();

    Ok(serde_json::json!({
        "report": report,
        "markdown": markdown,
        "extracted_text": extraction.text,
        "run_id": finished_run_id,
    }))
}

// --- Resume / partial re-run (1.3.2) ---

/// Re-run a past run, reusing its cached extraction and orientation and (for
/// partial modes) its successful step outputs, so only the necessary steps
/// re-execute. Uses the *active* profile's steps, so editing a prompt and
/// re-running is cheap. Modes:
///   - `from_step = Some(id)`: reuse steps before `id`; re-run `id` onward.
///   - `only_failed = true`: reuse successful steps except those downstream of
///     a failed step.
///   - neither: reuse only extraction/orientation; re-run every step.
#[tauri::command]
pub async fn rerun_run(
    app: AppHandle,
    run_id: String,
    from_step: Option<String>,
    only_failed: bool,
) -> Result<serde_json::Value, String> {
    if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A pipeline is already running".into());
    }
    let _guard = PipelineGuard;
    let bus = crate::emit::from_app(app);
    rerun_run_inner(&bus, &run_id, from_step, only_failed).await
}

fn read_run_file(run_id: &str, rel: &str) -> Result<String, String> {
    crate::runs::validate_run_id(run_id)?;
    let path = crate::runs::runs_dir()?.join(run_id).join(rel);
    std::fs::read_to_string(&path).map_err(|e| format!("Cannot read {rel} from run: {e}"))
}

fn load_run_report(run_id: &str) -> Result<PipelineReport, String> {
    let json = read_run_file(run_id, "report.json")
        .map_err(|_| "This run predates comparison support (no report.json).".to_string())?;
    serde_json::from_str(&json).map_err(|e| format!("Invalid report.json: {e}"))
}

async fn rerun_run_inner(
    app: &crate::emit::EventBus,
    parent_run_id: &str,
    from_step: Option<String>,
    only_failed: bool,
) -> Result<serde_json::Value, String> {
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::Release);
    crate::pipeline::logging::reset_run_usage();
    let start = std::time::Instant::now();

    let parent = crate::runs::load_manifest(parent_run_id)?;
    let report_json = read_run_file(parent_run_id, "report.json")
        .map_err(|_| "This run predates re-run support (no report.json). Re-run is only available for runs created after upgrading.".to_string())?;
    let parent_report: PipelineReport = serde_json::from_str(&report_json)
        .map_err(|e| format!("Invalid parent report.json: {e}"))?;
    let extracted_text = read_run_file(parent_run_id, "context/extracted_text.md")?;
    let orientation_value: serde_json::Value = parent_report.orientation.clone();

    // Read sandbox: the original input's directory.
    let source_path = parent.input_path.clone();
    let source_dir = {
        let p = std::path::Path::new(&source_path);
        if p.is_dir() { source_path.clone() } else {
            p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default()
        }
    };
    crate::pipeline::api_common::set_allowed_dirs(vec![source_dir]);

    // Active profile drives the re-run (edited prompts take effect).
    let config = pipeline_config::load();

    // Determine which steps to re-run vs. reuse.
    let enabled_ids: Vec<String> = config.steps.iter().filter(|s| s.enabled).map(|s| s.id.clone()).collect();
    let rerun: std::collections::HashSet<String> = if let Some(fs) = &from_step {
        match enabled_ids.iter().position(|id| id == fs) {
            Some(k) => enabled_ids[k..].iter().cloned().collect(),
            None => enabled_ids.iter().cloned().collect(), // unknown step → full re-run
        }
    } else if only_failed {
        let seeds: std::collections::HashSet<String> =
            parent_report.failed_steps.iter().map(|f| f.step_id.clone()).collect();
        let mut set = seeds.clone();
        set.extend(crate::pipeline::executor::dependents_of(&config, &seeds));
        set
    } else {
        enabled_ids.iter().cloned().collect()
    };

    // Preload the reused steps' outputs (keyed by base id).
    let mut preloaded: std::collections::HashMap<String, crate::models::StepOutput> = std::collections::HashMap::new();
    for o in &parent_report.step_outputs {
        if o.skipped { continue; }
        let base = o.step_id.split('/').next().unwrap_or(&o.step_id).to_string();
        if !rerun.contains(&base) {
            preloaded.insert(base, o.clone());
        }
    }
    app.emit_event("pipeline:log", serde_json::json!({
        "line": format!("Re-run of {parent_run_id}: reusing {} step(s), re-running the rest", preloaded.len())
    })).ok();

    // New run directory.
    let run_id = format!("{}_{}", parent_report.paper_hash, chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let mut run_writer = crate::runs::RunWriter::create(&run_id).ok();
    let artifact_write_dir: Option<String> = run_writer.as_ref().and_then(|w| {
        let dir = w.dir().join("artifacts");
        std::fs::create_dir_all(&dir).ok().map(|_| dir.to_string_lossy().replace('\\', "/"))
    });
    crate::pipeline::api_common::set_write_dir(artifact_write_dir.as_ref().map(std::path::PathBuf::from));
    if let Some(w) = run_writer.as_mut() {
        let _ = w.add_text("context/extracted_text.md", "Extracted text", "context", &extracted_text);
        let orient_json = serde_json::to_string_pretty(&orientation_value).unwrap_or_default();
        let _ = w.add_text("context/orientation.json", "Orientation map", "context", &orient_json);
        let logs_dir = w.dir().join("logs");
        if let Ok(f) = std::fs::create_dir_all(&logs_dir).and_then(|_| std::fs::File::create(logs_dir.join("run.log"))) {
            crate::pipeline::logging::set_log_sink(Some(f));
        }
    }

    // Extracted-text + orientation temp files for the steps to Read.
    let mut text_tmp = tempfile::Builder::new().prefix("pipeline_paper_").suffix(".txt").tempfile()
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    text_tmp.write_all(extracted_text.as_bytes()).map_err(|e| format!("Failed to write temp file: {e}"))?;
    text_tmp.flush().ok();
    let paper_text_path = text_tmp.path().to_string_lossy().replace('\\', "/");

    let orient_json = serde_json::to_string(&orientation_value).unwrap_or_default();
    let mut orient_tmp = tempfile::Builder::new().prefix("pipeline_orient_").suffix(".json").tempfile()
        .map_err(|e| format!("Failed to create orientation temp file: {e}"))?;
    orient_tmp.write_all(orient_json.as_bytes()).map_err(|e| format!("Failed to write orientation temp file: {e}"))?;
    orient_tmp.flush().ok();
    let orientation_path = orient_tmp.path().to_string_lossy().replace('\\', "/");

    let paper_type = crate::models::paper_view(&orientation_value)
        .map(|v| v.metadata.paper_type.to_string())
        .unwrap_or_default();
    let survey_hint = crate::models::survey_hint(&orientation_value);
    let variables = parent.variables.clone();

    let result = executor::execute_steps(
        app,
        &config,
        &orientation_path,
        &orientation_value,
        &paper_text_path,
        &source_path,
        &paper_type,
        &survey_hint,
        &variables,
        &std::collections::HashMap::new(),
        &preloaded,
        artifact_write_dir.as_deref(),
    )
    .await?;
    crate::pipeline::api_common::set_write_dir(None);
    if is_cancelled() { return Err("Pipeline cancelled".into()); }

    let report = PipelineReport {
        orientation: orientation_value,
        step_outputs: result.outputs,
        failed_steps: result.failed_steps,
        referee_reports: vec![],
        editor: None,
        report_date: chrono::Local::now().date_naive(),
        paper_hash: parent_report.paper_hash.clone(),
    };
    let elapsed = start.elapsed();
    let settings = crate::settings::load();
    let markdown = output::render_markdown(&report, None, elapsed, &settings);

    let profile_name = pipeline_config::load_profile(&settings.active_profile).map(|p| p.name).unwrap_or_default();
    let status = if report.failed_steps.is_empty() { "done" } else { "partial" };
    let meta = crate::runs::RunFinishMeta {
        input_path: parent.input_path.clone(),
        input_mode: parent.input_mode.clone(),
        profile_id: settings.active_profile.clone(),
        profile_name,
        provider: settings.preferred_provider.clone(),
        status: status.to_string(),
        duration_secs: elapsed.as_secs(),
        usage: crate::pipeline::logging::run_usage(),
        step_count: report.all_outputs().len() as u32,
        failed_steps: report.failed_steps.iter().map(|f| f.step_label.clone()).collect(),
        variables,
        parent_run_id: Some(parent_run_id.to_string()),
    };
    let mut finished_run_id: Option<String> = None;
    if let Some(w) = run_writer.take() {
        finished_run_id = finalize_run(app, w, &report, &markdown, meta);
    }
    enforce_retention(app, settings.max_saved_runs as usize);
    app.emit_event("pipeline:stage", serde_json::json!({"stage": "done"})).ok();

    Ok(serde_json::json!({
        "report": report,
        "markdown": markdown,
        "extracted_text": extracted_text,
        "run_id": finished_run_id,
    }))
}

// --- Run artifacts ---

#[tauri::command]
pub async fn get_run_manifest(run_id: String) -> Result<crate::runs::RunManifest, String> {
    crate::runs::load_manifest(&run_id)
}

#[tauri::command]
pub async fn read_artifact(
    run_id: String,
    rel_path: String,
) -> Result<crate::runs::ArtifactContent, String> {
    crate::runs::read_artifact(&run_id, &rel_path)
}

/// The structured report for a past run (for run-vs-run comparison).
#[tauri::command]
pub async fn get_run_report(run_id: String) -> Result<PipelineReport, String> {
    load_run_report(&run_id)
}

/// LLM reconciliation of two runs: which concerns were addressed, which remain,
/// what's new. Orders the two by creation time (older = "prior"). Guards
/// against a concurrent pipeline run.
#[tauri::command]
pub async fn reconcile_runs(app: AppHandle, run_a: String, run_b: String) -> Result<String, String> {
    if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A pipeline is already running".into());
    }
    let _guard = PipelineGuard;
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::Release);

    let report_a = load_run_report(&run_a)?;
    let report_b = load_run_report(&run_b)?;
    // Older run is the "prior"; fall back to the given order if timestamps tie.
    let (prior, current) = match (
        crate::runs::load_manifest(&run_a).ok(),
        crate::runs::load_manifest(&run_b).ok(),
    ) {
        (Some(ma), Some(mb)) if mb.created < ma.created => (&report_b, &report_a),
        _ => (&report_a, &report_b),
    };
    let bus = crate::emit::from_app(app);
    reconcile::reconcile(&bus, prior, current).await
}

/// List past runs (newest first) for the history page.
#[tauri::command]
pub async fn list_runs() -> Result<Vec<crate::runs::RunSummary>, String> {
    tokio::task::spawn_blocking(crate::runs::list_runs)
        .await
        .map_err(|e| format!("Run listing task failed: {e}"))?
}

/// Rename / retag a past run.
#[tauri::command]
pub async fn update_run_meta(run_id: String, title: String, tags: Vec<String>) -> Result<(), String> {
    crate::runs::update_run_meta(&run_id, &title, &tags)
}

/// Delete a past run and all its artifacts.
#[tauri::command]
pub async fn delete_run(run_id: String) -> Result<(), String> {
    crate::runs::delete_run(&run_id)
}

/// Number of runs on disk and total bytes they occupy.
#[tauri::command]
pub async fn runs_disk_usage() -> Result<crate::runs::RunsDiskUsage, String> {
    tokio::task::spawn_blocking(crate::runs::disk_usage)
        .await
        .map_err(|e| format!("Disk-usage task failed: {e}"))?
}

/// Delete the oldest runs beyond `keep` (0 = keep all). Returns how many were removed.
#[tauri::command]
pub async fn purge_runs(keep: u32) -> Result<usize, String> {
    crate::runs::purge_old_runs(keep as usize)
}

/// Write arbitrary text to a path (used by "Save console to file").
#[tauri::command]
pub async fn save_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("Failed to write file: {e}"))
}

/// Read a run's per-issue annotations (JSON string, "{}" if none).
#[tauri::command]
pub async fn get_annotations(run_id: String) -> Result<String, String> {
    crate::runs::read_annotations(&run_id)
}

/// Extract (id, title, body) triples from any issues-shaped step output in a
/// report — mirrors the frontend's `parseIssues`.
fn extract_issues_from_report(report: &PipelineReport) -> Vec<(String, String, String)> {
    for output in report.step_outputs.iter().rev() {
        if output.skipped {
            continue;
        }
        let Some(value) = crate::pipeline::structured::extract_json(&output.raw_text) else {
            continue;
        };
        let arr = if let Some(a) = value.as_array() {
            a.clone()
        } else if let Some(a) = value.get("issues").and_then(|v| v.as_array()) {
            a.clone()
        } else {
            continue;
        };
        let mut issues = Vec::new();
        for (i, item) in arr.iter().enumerate() {
            let Some(obj) = item.as_object() else { continue };
            let id = obj.get("id").and_then(|v| v.as_str()).map(|s| s.to_string())
                .unwrap_or_else(|| (i + 1).to_string());
            let title = obj.get("title").or_else(|| obj.get("summary"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            let body = obj.get("body").or_else(|| obj.get("description"))
                .and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !title.is_empty() || !body.is_empty() {
                issues.push((id, title, body));
            }
        }
        if !issues.is_empty() {
            return issues;
        }
    }
    Vec::new()
}

/// Collect the issues a reviewer rejected (annotation status "reject") across
/// all runs of `profile_id`, as short "title — body" lines. Capped.
fn collect_rejected_issues(profile_id: &str) -> Vec<String> {
    let runs = crate::runs::list_runs().unwrap_or_default();
    let mut out = Vec::new();
    for r in runs.iter().filter(|r| r.profile_id == profile_id) {
        let ann_str = crate::runs::read_annotations(&r.run_id).unwrap_or_else(|_| "{}".to_string());
        let ann: serde_json::Value = serde_json::from_str(&ann_str).unwrap_or(serde_json::json!({}));
        let Some(obj) = ann.as_object() else { continue };
        let rejected: Vec<String> = obj
            .iter()
            .filter(|(_, v)| v.get("status").and_then(|s| s.as_str()) == Some("reject"))
            .map(|(k, _)| k.clone())
            .collect();
        if rejected.is_empty() {
            continue;
        }
        let Ok(report) = load_run_report(&r.run_id) else { continue };
        let issues = extract_issues_from_report(&report);
        for id in rejected {
            if let Some((_, title, body)) = issues.iter().find(|(iid, _, _)| *iid == id) {
                let mut snippet = body.replace('\n', " ");
                if snippet.chars().count() > 240 {
                    snippet = snippet.chars().take(240).collect::<String>() + "…";
                }
                out.push(format!("- {title} — {snippet}"));
            }
        }
        if out.len() >= 40 {
            break;
        }
    }
    out
}

/// Draft a calibration addendum for the active profile's synthesis step from
/// the issues the reviewer has rejected, so the reviewer stops flagging them.
/// Returns the drafted text plus which step it targets. Guards against a
/// concurrent run (it makes one LLM call).
#[tauri::command]
pub async fn draft_calibration(app: AppHandle) -> Result<serde_json::Value, String> {
    let settings = crate::settings::load();
    let config = pipeline_config::load();
    let rejected = collect_rejected_issues(&settings.active_profile);
    if rejected.is_empty() {
        return Err("No rejected issues found for this profile yet. Reject some issues in the Issues view first.".into());
    }
    let target = config
        .steps
        .iter()
        .rev()
        .find(|s| s.enabled && s.phase == pipeline_config::Phase::Sequential)
        .ok_or("This profile has no sequential (synthesis) step to calibrate.")?;

    if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A pipeline is already running".into());
    }
    let _guard = PipelineGuard;
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::Release);

    let list = rejected.join("\n");
    let prompt = format!(
        "A reviewer rejected the following issues from past reviews as not worth flagging:\n\n{list}\n\n\
         Write 2–4 sentences to append to a review-consolidation prompt that instruct the reviewer to stop \
         flagging issues of these kinds in future. Identify the shared patterns concretely (topic, severity, \
         or type) rather than listing the specific items. Output only the sentences — no preamble, no headings."
    );
    let timeout = settings.step_timeout_secs.max(60);
    let bus = crate::emit::from_app(app);
    let raw = crate::pipeline::claude::call_llm(
        &bus, &prompt, &[], None, "text", timeout, "Prompt calibration", None, None, &[],
        &crate::pipeline::claude::LlmOverrides::default(),
    )
    .await?;
    let addendum = output::strip_to_report(&raw);

    Ok(serde_json::json!({
        "addendum": addendum.trim(),
        "target_step_id": target.id,
        "target_label": target.label,
        "rejected_count": rejected.len(),
    }))
}

/// Save a run's per-issue annotations (validated JSON).
#[tauri::command]
pub async fn save_annotations(run_id: String, content: String) -> Result<(), String> {
    crate::runs::write_annotations(&run_id, &content)
}

#[tauri::command]
pub async fn cancel_pipeline() -> Result<(), String> {
    CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
    kill_all_children();
    Ok(())
}

// --- Batch queue ---

/// Start a batch: run each input path through the pipeline sequentially with
/// the active profile and (optionally) shared variable values. Returns
/// immediately; progress arrives via `batch:progress` events and
/// `get_batch_status`. Fails if any run (single or batch) is already active.
#[tauri::command]
pub async fn start_batch(
    app: AppHandle,
    paths: Vec<String>,
    variables: Option<std::collections::HashMap<String, String>>,
) -> Result<(), String> {
    let paths: Vec<String> = paths.into_iter().filter(|p| !p.trim().is_empty()).collect();
    if paths.is_empty() {
        return Err("No inputs to run".into());
    }
    if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A pipeline is already running".into());
    }
    BATCH_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    {
        let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
        *jobs = paths
            .iter()
            .map(|p| BatchJob {
                path: p.clone(),
                name: basename(p),
                status: "pending".to_string(),
                run_id: None,
                error: None,
                duration_secs: 0,
            })
            .collect();
    }
    let vars = variables.unwrap_or_default();
    let bus = crate::emit::from_app(app);
    emit_batch(&bus);

    tauri::async_runtime::spawn(async move {
        // RAII guard clears PIPELINE_RUNNING and per-run state even on panic.
        let _guard = PipelineGuard;
        for i in 0..paths.len() {
            if BATCH_CANCEL.load(std::sync::atomic::Ordering::Acquire) {
                mark_remaining_cancelled(i);
                break;
            }
            set_job(i, |j| j.status = "running".to_string());
            emit_batch(&bus);

            let started = std::time::Instant::now();
            let result = run_pipeline_inner(&bus, &paths[i], false, vars.clone(), Default::default()).await;
            let secs = started.elapsed().as_secs();

            match result {
                Ok(v) => {
                    let run_id = v.get("run_id").and_then(|r| r.as_str()).map(|s| s.to_string());
                    set_job(i, |j| {
                        j.status = "done".to_string();
                        j.run_id = run_id.clone();
                        j.duration_secs = secs;
                    });
                }
                Err(e) => {
                    let cancelled = e.to_lowercase().contains("cancelled");
                    set_job(i, |j| {
                        j.status = if cancelled { "cancelled" } else { "failed" }.to_string();
                        j.error = Some(e.clone());
                        j.duration_secs = secs;
                    });
                }
            }
            emit_batch(&bus);
        }
        let _ = bus.emit_event("batch:done", serde_json::Value::Null);
    });

    Ok(())
}

fn set_job(i: usize, f: impl FnOnce(&mut BatchJob)) {
    let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(j) = jobs.get_mut(i) {
        f(j);
    }
}

fn mark_remaining_cancelled(from: usize) {
    let mut jobs = BATCH.lock().unwrap_or_else(|e| e.into_inner());
    for j in jobs.iter_mut().skip(from) {
        if j.status == "pending" || j.status == "running" {
            j.status = "cancelled".to_string();
        }
    }
}

/// Current batch job list (empty if no batch has run this session).
#[tauri::command]
pub async fn get_batch_status() -> Result<Vec<BatchJob>, String> {
    Ok(BATCH.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

/// Cancel a running batch: stop the current run and skip the rest.
#[tauri::command]
pub async fn cancel_batch() -> Result<(), String> {
    BATCH_CANCEL.store(true, std::sync::atomic::Ordering::Release);
    CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
    kill_all_children();
    Ok(())
}

/// Input files (PDF/LaTeX) directly under `dir`, non-recursive and sorted;
/// hidden files skipped. Shared by "queue this folder" and the watcher.
fn scan_input_files(dir: &str) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Cannot read folder: {e}"))?;
    let mut files: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(ext.as_str(), "pdf" | "tex") {
            files.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(files)
}

/// List input files (PDF/LaTeX) directly under `dir`, for "queue this folder".
#[tauri::command]
pub async fn list_input_files(dir: String) -> Result<Vec<String>, String> {
    scan_input_files(&dir)
}

// --- Watch folder ---

fn emit_watch(app: &crate::emit::EventBus) {
    let state = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let _ = app.emit_event("watch:status", serde_json::to_value(&state).unwrap_or_default());
}

/// Start watching `folder`: files that appear from now on are run through the
/// pipeline with the active profile, one at a time. Files already present are
/// treated as the baseline and not run.
#[tauri::command]
pub async fn start_watch(app: AppHandle, folder: String) -> Result<(), String> {
    if folder.trim().is_empty() {
        return Err("No folder to watch".into());
    }
    scan_input_files(&folder)?; // validate readable
    if WATCH_ACTIVE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Already watching a folder".into());
    }
    {
        let mut st = WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner());
        *st = WatchStatus { active: true, folder: folder.clone(), processed: Vec::new() };
    }
    let bus = crate::emit::from_app(app);
    emit_watch(&bus);

    tauri::async_runtime::spawn(async move {
        // Baseline: files already present are not (re)processed.
        let mut seen: std::collections::HashSet<String> =
            scan_input_files(&folder).unwrap_or_default().into_iter().collect();
        while WATCH_ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if !WATCH_ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            let current = match scan_input_files(&folder) {
                Ok(f) => f,
                Err(_) => continue,
            };
            for path in current {
                if seen.contains(&path) {
                    continue;
                }
                // Skip this cycle if any run is active; retry next poll (don't
                // mark as seen, so it's picked up once free).
                if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                seen.insert(path.clone());
                let guard = PipelineGuard;
                let name = basename(&path);
                let result = run_pipeline_inner(&bus, &path, false, Default::default(), Default::default()).await;
                drop(guard);
                let job = match result {
                    Ok(v) => BatchJob {
                        path: path.clone(),
                        name,
                        status: "done".to_string(),
                        run_id: v.get("run_id").and_then(|r| r.as_str()).map(|s| s.to_string()),
                        error: None,
                        duration_secs: 0,
                    },
                    Err(e) => BatchJob {
                        path: path.clone(),
                        name,
                        status: "failed".to_string(),
                        run_id: None,
                        error: Some(e),
                        duration_secs: 0,
                    },
                };
                WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner()).processed.push(job);
                emit_watch(&bus);
                if !WATCH_ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
            }
        }
        WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner()).active = false;
        emit_watch(&bus);
    });
    Ok(())
}

/// Stop watching (the current file, if any, finishes first).
#[tauri::command]
pub async fn stop_watch() -> Result<(), String> {
    WATCH_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner()).active = false;
    Ok(())
}

#[tauri::command]
pub async fn get_watch_status() -> Result<WatchStatus, String> {
    Ok(WATCH_STATE.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

// --- File I/O ---

#[tauri::command]
pub async fn save_report_md(path: String, markdown: String) -> Result<(), String> {
    std::fs::write(&path, &markdown).map_err(|e| format!("Failed to write file: {e}"))
}

/// Save all pipeline artifacts to a directory.
#[tauri::command]
pub async fn save_all_artifacts(
    dir: String,
    markdown: String,
    extracted_text: String,
    report: crate::models::PipelineReport,
) -> Result<(), String> {
    let base = std::path::Path::new(&dir);
    std::fs::create_dir_all(base).map_err(|e| format!("Failed to create directory: {e}"))?;

    // Final report
    std::fs::write(base.join("report.md"), &markdown)
        .map_err(|e| format!("Failed to write report.md: {e}"))?;

    // Extracted text
    std::fs::write(base.join("extracted_text.md"), &extracted_text)
        .map_err(|e| format!("Failed to write extracted_text.md: {e}"))?;

    // Orientation map
    let orient_json = serde_json::to_string_pretty(&report.orientation)
        .map_err(|e| format!("Failed to serialize orientation: {e}"))?;
    std::fs::write(base.join("orientation.json"), &orient_json)
        .map_err(|e| format!("Failed to write orientation.json: {e}"))?;

    // Individual step outputs
    let steps_dir = base.join("steps");
    std::fs::create_dir_all(&steps_dir)
        .map_err(|e| format!("Failed to create steps directory: {e}"))?;

    let outputs = report.all_outputs();
    for (i, output) in outputs.iter().enumerate() {
        let slug = output.step_id
            .replace('/', "_")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect::<String>();
        let filename = format!("{:02}_{}.md", i + 1, slug);
        let header = format!("# {}\n\n**Phase**: {} · **Agent**: {}\n\n---\n\n",
            output.step_label,
            output.phase,
            if output.agent.is_empty() { "default" } else { &output.agent },
        );
        std::fs::write(steps_dir.join(&filename), format!("{}{}", header, output.raw_text))
            .map_err(|e| format!("Failed to write {filename}: {e}"))?;
    }

    Ok(())
}

/// Stash the last export path so we can clean it up on the next export.
static LAST_EXPORT_PATH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[tauri::command]
pub async fn print_report_html(markdown: String) -> Result<(), String> {
    use pulldown_cmark::{Parser, Options, html};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(&markdown, options);
    let mut html_body = String::new();
    html::push_html(&mut html_body, parser);

    // Sanitize HTML to strip <script>, event handlers, and other XSS vectors
    // that could be injected via LLM output (e.g. prompt injection in paper text).
    // ammonia's defaults allow all standard text/table/list elements that
    // pulldown-cmark produces, while stripping dangerous content.
    let html_body = ammonia::clean(&html_body);

    // KaTeX resources inlined at compile time so export works offline.
    // Font URLs are rewritten to absolute CDN paths — fonts load when
    // online but math still renders (with fallback fonts) when offline.
    const KATEX_CSS: &str = include_str!("../../node_modules/katex/dist/katex.min.css");
    const KATEX_JS: &str = include_str!("../../node_modules/katex/dist/katex.min.js");
    const AUTO_RENDER_JS: &str = include_str!("../../node_modules/katex/dist/contrib/auto-render.min.js");

    let katex_css = KATEX_CSS.replace(
        "url(fonts/",
        "url(https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/fonts/",
    );

    let mut html_doc = String::with_capacity(
        katex_css.len() + KATEX_JS.len() + AUTO_RENDER_JS.len() + html_body.len() + 2048,
    );
    html_doc.push_str("<!DOCTYPE html>\n<html><head>\n<meta charset=\"utf-8\">\n<title>Pipeline Report</title>\n<style>\n");
    html_doc.push_str(&katex_css);
    html_doc.push_str("\n</style>\n<script>\n");
    html_doc.push_str(KATEX_JS);
    html_doc.push_str("\n</script>\n<script>\n");
    html_doc.push_str(AUTO_RENDER_JS);
    html_doc.push_str(concat!(
        "\n</script>\n<style>\n",
        "body { font-family: \"Times New Roman\", Times, serif; max-width: 48em; margin: 2em auto; padding: 0 1em; line-height: 1.5; color: #111; }\n",
        "h1 { font-size: 1.4em; } h2 { font-size: 1.2em; border-bottom: 1px solid #ccc; padding-bottom: 0.2em; }\n",
        "h3 { font-size: 1.05em; } hr { border: none; border-top: 1px solid #ddd; margin: 1.5em 0; }\n",
        "table { border-collapse: collapse; width: 100%; margin: 1em 0; }\n",
        "th, td { border: 1px solid #ccc; padding: 0.4em 0.6em; text-align: left; }\n",
        "th { background: #f5f5f5; }\n",
        "code { background: #f4f4f4; padding: 0.1em 0.3em; border-radius: 3px; font-size: 0.9em; }\n",
        "pre { background: #f4f4f4; padding: 1em; overflow-x: auto; border-radius: 4px; }\n",
        "pre code { background: none; padding: 0; }\n",
        "blockquote { border-left: 3px solid #ccc; margin: 1em 0; padding: 0.5em 1em; color: #555; }\n",
        "@media print { body { margin: 0; max-width: none; } }\n",
        "</style>\n</head><body>\n",
    ));
    html_doc.push_str(&html_body);
    // KaTeX JS is inline (synchronous), so renderMathInElement is available
    // immediately. requestAnimationFrame ensures the browser repaints with
    // rendered math before the print dialog opens.
    html_doc.push_str(concat!(
        "\n<script>",
        "renderMathInElement(document.body,{delimiters:[",
        "{left:'$$',right:'$$',display:true},",
        "{left:'$',right:'$',display:false},",
        "{left:'\\\\(',right:'\\\\)',display:false},",
        "{left:'\\\\[',right:'\\\\]',display:true}",
        "]});",
        "requestAnimationFrame(function(){window.print();});",
        "</script>\n</body></html>",
    ));

    // Clean up previous export file
    if let Ok(mut prev) = LAST_EXPORT_PATH.lock() {
        if let Some(old_path) = prev.take() {
            let _ = std::fs::remove_file(&old_path);
        }
    }

    let mut tmp = tempfile::Builder::new()
        .prefix("pipeline_report_")
        .suffix(".html")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    tmp.write_all(html_doc.as_bytes())
        .map_err(|e| format!("Failed to write HTML: {e}"))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush: {e}"))?;

    let path = tmp.into_temp_path();
    let path_str = path.to_string_lossy().to_string();
    path.keep().map_err(|e| format!("Failed to persist temp file: {e}"))?;

    if let Ok(mut prev) = LAST_EXPORT_PATH.lock() {
        *prev = Some(path_str.clone());
    }

    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg("--").arg(&path_str).spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(&path_str)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg("--").arg(&path_str).spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;

    Ok(())
}

/// Open ~/.pipeline/ in the OS file manager. The path is resolved server-side
/// (never passed from the frontend) so there is nothing to sanitize.
#[tauri::command]
pub async fn open_pipeline_dir() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create .pipeline dir: {e}"))?;
    let path_str = dir.to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg("--").arg(&path_str).spawn()
        .map_err(|e| format!("Failed to open folder: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(&path_str)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg("--").arg(&path_str).spawn()
        .map_err(|e| format!("Failed to open folder: {e}"))?;

    Ok(())
}

// --- History / Settings ---

#[derive(serde::Serialize)]
pub struct HistoryResponse {
    pub reports: Vec<ReportSummary>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn list_history() -> Result<HistoryResponse, String> {
    let result = storage::list_reports()?;
    Ok(HistoryResponse {
        reports: result.summaries,
        warnings: result.warnings,
    })
}

#[tauri::command]
pub async fn check_deps() -> Result<crate::deps::DepsReport, String> {
    tokio::task::spawn_blocking(crate::deps::check_all)
        .await
        .map_err(|e| format!("Dependency check failed: {e}"))
}

#[derive(serde::Serialize)]
pub struct SettingsResponse {
    pub settings: crate::settings::Settings,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn get_settings() -> Result<SettingsResponse, String> {
    let (settings, warnings) = crate::settings::load_with_warnings();
    Ok(SettingsResponse { settings, warnings })
}

#[tauri::command]
pub async fn save_settings(settings: crate::settings::Settings) -> Result<(), String> {
    crate::settings::save(&settings)
}

// --- Pipeline config ---

#[tauri::command]
pub async fn get_pipeline_config() -> Result<PipelineConfig, String> {
    Ok(pipeline_config::load())
}

#[tauri::command]
pub async fn save_pipeline_config(config: PipelineConfig) -> Result<(), String> {
    pipeline_config::save(&config)
}

#[tauri::command]
pub async fn get_default_parallel_template() -> String {
    pipeline_config::default_parallel_template()
}

/// Compiled-in default text for a named prompt (no user overrides applied).
/// Backs the editor's "reset to …" actions, e.g. the generic vs paper-review
/// context templates and survey prompts.
#[tauri::command]
pub async fn get_default_prompt(name: String) -> Result<String, String> {
    crate::prompts::compiled_default(&name)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Unknown prompt: {name}"))
}

#[tauri::command]
pub async fn reset_pipeline_config() -> Result<PipelineConfig, String> {
    Ok(pipeline_config::reset_defaults())
}

// --- Profiles ---

#[tauri::command]
pub async fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    pipeline_config::list_profiles()
}

#[tauri::command]
pub async fn get_active_profile() -> Result<String, String> {
    Ok(pipeline_config::get_active_profile_id())
}

#[tauri::command]
pub async fn create_profile(name: String) -> Result<ProfileSummary, String> {
    pipeline_config::create_profile(&name)
}

#[tauri::command]
pub async fn duplicate_profile(source_id: String, new_name: String) -> Result<ProfileSummary, String> {
    pipeline_config::duplicate_profile(&source_id, &new_name)
}

#[tauri::command]
pub async fn rename_profile(id: String, new_name: String) -> Result<ProfileSummary, String> {
    pipeline_config::rename_profile(&id, &new_name)
}

#[tauri::command]
pub async fn delete_profile(id: String) -> Result<(), String> {
    pipeline_config::delete_profile(&id)
}

#[tauri::command]
pub async fn switch_profile(id: String) -> Result<PipelineConfig, String> {
    pipeline_config::switch_profile(&id)
}

#[tauri::command]
pub async fn export_item(path: String, json: String) -> Result<(), String> {
    std::fs::write(&path, json).map_err(|e| format!("Failed to write: {e}"))
}

#[tauri::command]
pub async fn import_item(path: String) -> Result<serde_json::Value, String> {
    let content = read_import_file(&path)?;
    let envelope = pipeline_config::import_envelope(&content)?;
    serde_json::to_value(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

#[tauri::command]
pub async fn export_profile(id: String, path: String) -> Result<(), String> {
    let json = pipeline_config::export_profile_data(&id)?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write: {e}"))
}

/// Import a profile from a parsed envelope, checking the schema version.
/// Shared by file and URL import.
fn import_profile_envelope(envelope: pipeline_config::ExportEnvelope) -> Result<ProfileSummary, String> {
    match envelope {
        pipeline_config::ExportEnvelope::Profile { schema_version, name, steps, merge, use_orientation, orientation_prompt, extraction, parallel_context_template, variables } => {
            if schema_version > pipeline_config::CURRENT_SCHEMA_VERSION {
                return Err(format!(
                    "This profile was made with a newer version of Pipeline (schema v{schema_version}). Update the app to import it."
                ));
            }
            pipeline_config::import_profile_data(&name, steps, merge, use_orientation, orientation_prompt, extraction, parallel_context_template, variables)
        }
        pipeline_config::ExportEnvelope::Step { .. } => {
            Err("This file contains a single step, not a profile. Use Import on the pipeline page to add it to the current profile.".into())
        }
        pipeline_config::ExportEnvelope::Bundle { .. } => {
            Err("This file is a full bundle. Use Import All to restore it.".into())
        }
    }
}

/// Fetch a profile JSON from an http(s) URL and import it. For sharing profiles
/// by link (a lab, a syllabus, a gist).
#[tauri::command]
pub async fn import_profile_from_url(url: String) -> Result<ProfileSummary, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("URL must start with https:// or http://".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;
    let resp = client
        .get(&url)
        .header("User-Agent", "pipeline")
        .send()
        .await
        .map_err(|e| format!("Fetch failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Fetch failed: HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("Fetch failed: {e}"))?;
    if bytes.len() as u64 > MAX_IMPORT_SIZE {
        return Err("The fetched file is too large to be a profile.".into());
    }
    let content = String::from_utf8_lossy(&bytes).to_string();
    let envelope = pipeline_config::import_envelope(&content)
        .map_err(|e| format!("The URL did not contain a valid profile: {e}"))?;
    import_profile_envelope(envelope)
}

#[tauri::command]
pub async fn import_profile(path: String) -> Result<ProfileSummary, String> {
    let content = read_import_file(&path)?;
    let envelope = pipeline_config::import_envelope(&content)?;
    import_profile_envelope(envelope)
}

#[tauri::command]
pub async fn export_bundle(path: String) -> Result<(), String> {
    let json = pipeline_config::export_bundle()?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write: {e}"))
}

#[tauri::command]
pub async fn import_bundle(path: String) -> Result<(), String> {
    let content = read_import_file(&path)?;
    pipeline_config::import_bundle(&content)
}

#[tauri::command]
pub async fn check_for_update() -> Result<crate::updates::UpdateInfo, String> {
    crate::updates::check().await
}

// ── Preprocessing artifact cache ────────────────────────────────────
//
// The extracted paper text is cached at ~/.pipeline/cache/papers/{hash}.txt
// so users can inspect the exact text the LLMs received, even after the
// pipeline run completes and the temp files are gone. Inspecting helps
// catch extraction failures (garbled equations, missing pages) before they
// confuse the referees.

fn paper_cache_dir() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline").join("cache").join("papers");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create cache dir {}: {e}", dir.display()))?;
    Ok(dir)
}

fn validate_paper_hash(hash: &str) -> Result<(), String> {
    if hash.is_empty() {
        return Err("Empty paper hash".into());
    }
    if !hash.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Invalid paper hash".into());
    }
    Ok(())
}

/// Write the extracted text to ~/.pipeline/cache/papers/{hash}.txt and return
/// the absolute path. Best-effort: callers should not abort on failure.
fn cache_paper_text(paper_hash: &str, text: &str) -> Result<String, String> {
    validate_paper_hash(paper_hash)?;
    let dir = paper_cache_dir()?;
    let path = dir.join(format!("{paper_hash}.txt"));
    std::fs::write(&path, text).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(path.to_string_lossy().replace('\\', "/"))
}

/// Read the cached extracted text for a given paper hash. Returns an empty
/// result with `cached: false` when the cache miss is expected (no prior run).
#[tauri::command]
pub async fn read_cached_paper_text(paper_hash: String) -> Result<serde_json::Value, String> {
    validate_paper_hash(&paper_hash)?;
    let dir = paper_cache_dir()?;
    let path = dir.join(format!("{paper_hash}.txt"));
    if !path.exists() {
        return Ok(serde_json::json!({ "cached": false, "text": "" }));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    Ok(serde_json::json!({ "cached": true, "text": text, "path": path.to_string_lossy() }))
}

// --- Managed local engines ---

/// Status of every installable engine. The disk-usage walk can touch
/// multi-GB trees, so it runs off the async runtime.
#[tauri::command]
pub async fn list_engines() -> Result<Vec<crate::engines::EngineStatus>, String> {
    tokio::task::spawn_blocking(crate::engines::engine_statuses)
        .await
        .map_err(|e| format!("Engine status task failed: {e}"))
}

#[tauri::command]
pub async fn install_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    crate::engines::install_engine(&app, &engine_id).await
}

#[tauri::command]
pub async fn uninstall_engine(app: AppHandle, engine_id: String) -> Result<(), String> {
    crate::engines::uninstall_engine(&app, &engine_id).await
}

#[tauri::command]
pub fn cancel_engine_install() {
    crate::engines::cancel_install();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_extracts_filename() {
        assert_eq!(basename("/papers/main.pdf"), "main.pdf");
        assert_eq!(basename("relative.tex"), "relative.tex");
        assert_eq!(basename(""), "");
    }

    // Exercises the batch job-status helpers over the global BATCH state. This
    // is the only test that touches BATCH, so parallel test runs can't race it.
    #[test]
    fn batch_helpers_update_and_cancel_jobs() {
        {
            let mut jobs = BATCH.lock().unwrap();
            *jobs = vec![
                BatchJob { path: "a".into(), name: "a".into(), status: "done".into(), run_id: None, error: None, duration_secs: 0 },
                BatchJob { path: "b".into(), name: "b".into(), status: "running".into(), run_id: None, error: None, duration_secs: 0 },
                BatchJob { path: "c".into(), name: "c".into(), status: "pending".into(), run_id: None, error: None, duration_secs: 0 },
            ];
        }
        set_job(1, |j| { j.status = "done".into(); j.run_id = Some("r1".into()); });
        mark_remaining_cancelled(1);
        let jobs = BATCH.lock().unwrap();
        assert_eq!(jobs[0].status, "done"); // untouched
        assert_eq!(jobs[1].status, "done"); // set_job ran before cancel; already terminal
        assert_eq!(jobs[1].run_id.as_deref(), Some("r1"));
        assert_eq!(jobs[2].status, "cancelled"); // pending → cancelled
    }
}
