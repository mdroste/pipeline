//! Tauri command handlers.

use crate::models::PipelineReport;
use crate::models::ReportSummary;
use crate::pipeline::{executor, extract, orient, reconcile};
use crate::pipeline_config::{self, PipelineConfig, ProfileSummary};
use crate::output;
use crate::storage;
use std::io::Write;
use tauri::{AppHandle, Emitter};

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
    }
}

/// PIDs of active child processes (claude/gemini/codex subprocesses).
/// Populated by `register_child_pid`, cleared by `unregister_child_pid`.
static CHILD_PIDS: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

pub fn is_cancelled() -> bool {
    CANCEL_FLAG.load(std::sync::atomic::Ordering::Acquire)
}

/// Register a child process PID so it can be killed on cancel.
pub fn register_child_pid(pid: u32) {
    let mut pids = CHILD_PIDS.lock().unwrap_or_else(|e| e.into_inner());
    pids.push(pid);
}

/// Unregister a child process PID after it exits.
pub fn unregister_child_pid(pid: u32) {
    let mut pids = CHILD_PIDS.lock().unwrap_or_else(|e| e.into_inner());
    pids.retain(|&p| p != pid);
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

#[tauri::command]
pub async fn run_pipeline(
    app: AppHandle,
    paper_path: String,
    diff: bool,
) -> Result<serde_json::Value, String> {
    // Prevent concurrent pipeline runs from corrupting shared state
    if PIPELINE_RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("A pipeline is already running".into());
    }
    // RAII guard ensures cleanup runs even on panic
    let _guard = PipelineGuard;
    run_pipeline_inner(&app, &paper_path, diff).await
}

async fn run_pipeline_inner(
    app: &AppHandle,
    paper_path: &str,
    diff: bool,
) -> Result<serde_json::Value, String> {
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::Release);
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
    crate::pipeline::api_common::set_allowed_dirs(vec![source_dir]);

    // Extract paper text
    app.emit("pipeline:stage", serde_json::json!({"stage": "extracting"})).ok();
    let extract_start = std::time::Instant::now();
    let extraction = extract::extract(app, paper_path).await?;
    let extract_secs = extract_start.elapsed().as_secs();
    app.emit("pipeline:log", serde_json::json!({
        "line": format!("Extracted via {} ({} chars, {}s)", extraction.method, extraction.text.len(), extract_secs)
    })).ok();
    for note in &extraction.quality_notes {
        app.emit("pipeline:log", serde_json::json!({
            "line": format!("WARNING: {note}")
        })).ok();
    }
    if is_cancelled() { return Err("Pipeline cancelled".into()); }

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

    let config = pipeline_config::load();

    // Build orientation map (optional)
    let mut _orient_tmp = None; // hold tempfile alive
    let orientation;
    let orientation_path;

    if config.use_orientation {
        app.emit("pipeline:stage", serde_json::json!({"stage": "orienting"})).ok();
        let orient_start = std::time::Instant::now();
        orientation = orient::build_orientation_map(app, &extraction).await?;
        let orient_secs = orient_start.elapsed().as_secs();
        app.emit("pipeline:log", serde_json::json!({
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
        let orient_bytes = orientation_json.len();
        app.emit("pipeline:log", serde_json::json!({
            "line": format!("Orientation map written to temp file ({orient_bytes} bytes)")
        })).ok();
        _orient_tmp = Some(orient_file);
    } else {
        app.emit("pipeline:log", serde_json::json!({
            "line": "Orientation map disabled for this profile"
        })).ok();
        orientation = crate::models::OrientationMap::empty(&extraction.text);
        orientation_path = String::new();
    }

    let paper_type = orientation.metadata.paper_type.to_string();

    let result = executor::execute_steps(
        app,
        &config,
        &orientation_path,
        &paper_text_path,
        &extraction.source_path,
        &paper_type,
    )
    .await?;

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
        let _ = app.emit(
            "pipeline:log",
            serde_json::json!({ "line": format!("WARNING: Failed to save report to history: {e}. The report is still available but won't appear in history or be available for diffing.") }),
        );
    }
    let elapsed = pipeline_start.elapsed();
    let settings = crate::settings::load();
    let markdown = output::render_markdown(&report, diff_text.as_deref(), elapsed, &settings);

    app.emit("pipeline:stage", serde_json::json!({"stage": "done"})).ok();

    Ok(serde_json::json!({
        "report": report,
        "markdown": markdown,
        "extracted_text": extraction.text,
    }))
}

#[tauri::command]
pub async fn cancel_pipeline() -> Result<(), String> {
    CANCEL_FLAG.store(true, std::sync::atomic::Ordering::Release);
    kill_all_children();
    Ok(())
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

#[tauri::command]
pub async fn import_profile(path: String) -> Result<ProfileSummary, String> {
    let content = read_import_file(&path)?;
    let envelope = pipeline_config::import_envelope(&content)?;
    match envelope {
        pipeline_config::ExportEnvelope::Profile { name, steps, merge, use_orientation, parallel_context_template } => {
            pipeline_config::import_profile_data(&name, steps, merge, use_orientation, parallel_context_template)
        }
        pipeline_config::ExportEnvelope::Step { .. } => {
            Err("This file contains a single step, not a profile. Use Import on the pipeline page to add it to the current profile.".into())
        }
        pipeline_config::ExportEnvelope::Bundle { .. } => {
            Err("This file is a full bundle. Use Import All to restore it.".into())
        }
    }
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
