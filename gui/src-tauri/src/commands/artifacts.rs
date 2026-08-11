use super::*;

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

#[tauri::command]
pub async fn read_pdf_artifact_page(
    run_id: String,
    rel_path: String,
    page: u32,
) -> Result<crate::runs::PdfArtifactPage, String> {
    tokio::task::spawn_blocking(move || {
        crate::runs::read_pdf_artifact_page(&run_id, &rel_path, page)
    })
    .await
    .map_err(|error| format!("PDF preview task failed: {error}"))?
}

#[tauri::command]
pub async fn read_page_artifact(
    run_id: String,
    page: u32,
) -> Result<crate::runs::ArtifactContent, String> {
    crate::runs::read_page_artifact(&run_id, page)
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
pub async fn reconcile_runs(
    app: AppHandle,
    run_a: String,
    run_b: String,
) -> Result<String, String> {
    let _guard = acquire_pipeline_guard()?;
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
    tokio::task::spawn_blocking(|| {
        // A cancelled run is finalized after the original command returns,
        // so History refresh is also a recovery point in the current process.
        let _ = crate::runs::recover_resumable_runs();
        crate::runs::list_runs()
    })
    .await
    .map_err(|e| format!("Run listing task failed: {e}"))?
}

/// Rename / retag a past run.
#[tauri::command]
pub async fn update_run_meta(
    run_id: String,
    title: String,
    tags: Vec<String>,
) -> Result<(), String> {
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

/// Preview the completed runs selected by the supplied retention limits.
/// This performs the same bounded disk scan as purge without deleting data.
#[tauri::command]
pub async fn preview_purge_runs(
    keep: u32,
    max_bytes: u64,
) -> Result<crate::runs::RunPurgePreview, String> {
    tokio::task::spawn_blocking(move || {
        crate::runs::preview_purge_runs_with_limits(keep as usize, max_bytes)
    })
    .await
    .map_err(|e| format!("Purge-preview task failed: {e}"))?
}

/// Apply configured count/byte history limits now. Refuses to run concurrently
/// with a pipeline so the active run cannot be selected for deletion.
#[tauri::command]
pub async fn purge_runs(keep: u32, max_bytes: u64, preview_token: String) -> Result<usize, String> {
    let _guard = acquire_pipeline_guard()?;
    crate::runs::purge_runs_with_expected_preview(keep as usize, max_bytes, &preview_token)
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
pub(super) fn extract_issues_from_report(report: &PipelineReport) -> Vec<(String, String, String)> {
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
            let Some(obj) = item.as_object() else {
                continue;
            };
            let id = obj
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| (i + 1).to_string());
            let title = obj
                .get("title")
                .or_else(|| obj.get("summary"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body = obj
                .get("body")
                .or_else(|| obj.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
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
pub(super) fn collect_rejected_issues(profile_id: &str) -> Vec<String> {
    let runs = crate::runs::list_runs().unwrap_or_default();
    let mut out = Vec::new();
    for r in runs.iter().filter(|r| r.profile_id == profile_id) {
        let ann_str = crate::runs::read_annotations(&r.run_id).unwrap_or_else(|_| "{}".to_string());
        let ann: serde_json::Value =
            serde_json::from_str(&ann_str).unwrap_or(serde_json::json!({}));
        let Some(obj) = ann.as_object() else { continue };
        let rejected: Vec<String> = obj
            .iter()
            .filter(|(_, v)| v.get("status").and_then(|s| s.as_str()) == Some("reject"))
            .map(|(k, _)| k.clone())
            .collect();
        if rejected.is_empty() {
            continue;
        }
        let Ok(report) = load_run_report(&r.run_id) else {
            continue;
        };
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
    let settings = crate::settings::load_persisted_required().map_err(|e| {
        format!("Cannot draft calibration because settings could not be loaded safely: {e}")
    })?;
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

    let _guard = acquire_pipeline_guard()?;
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
    let mut request = crate::pipeline::call::OwnedRequest::new(
        &bus,
        "calibration",
        "Prompt calibration",
        prompt,
        timeout,
    );
    request.settings = std::sync::Arc::new(settings);
    let raw = crate::pipeline::call::execute_text(request).await?;
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
    signal_cancellation();
    kill_all_children();
    Ok(())
}
