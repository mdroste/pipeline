use super::*;

/// Create the optional persistent workspace shared by fresh runs and re-runs.
/// Persistence remains best-effort; execution falls back to stdout when the
/// run or artifact directory cannot be created.
pub(super) fn create_run_workspace(
    app: &crate::emit::EventBus,
    paper_hash: &str,
    pending_meta: crate::runs::RunFinishMeta,
    preprocessing_log: Option<&std::path::Path>,
) -> (Option<crate::runs::RunWriter>, Option<String>) {
    let mut writer = match crate::runs::RunWriter::create_unique(paper_hash) {
        Ok(writer) => Some(writer),
        Err(error) => {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: could not create run directory: {error}")
                }),
            );
            None
        }
    };

    if let Some(writer) = writer.as_mut() {
        if let Err(error) = writer.set_pending_meta(pending_meta) {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: could not update pending run manifest: {error}")
                }),
            );
        }
    }

    if let Some(writer) = writer.as_ref() {
        let logs_dir = writer.dir().join("logs");
        let log_path = logs_dir.join("run.log");
        let opened = std::fs::create_dir_all(&logs_dir).and_then(|()| {
            if let Some(source) = preprocessing_log {
                std::fs::copy(source, &log_path)?;
            }
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
        });
        match opened {
            Ok(file) => crate::pipeline::logging::set_log_sink(Some(file)),
            Err(error) => {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!("WARNING: could not open run log file: {error}")
                    }),
                );
            }
        }
    }

    let artifact_dir = writer.as_ref().and_then(|writer| {
        let dir = writer.dir().join("artifacts");
        match std::fs::create_dir_all(&dir) {
            Ok(()) => Some(
                crate::pipeline::claude::normalize_cli_root(&dir.to_string_lossy())
                    .unwrap_or_else(|| dir.to_string_lossy().replace('\\', "/")),
            ),
            Err(error) => {
                let _ = app.emit_event(
                    "pipeline:log",
                    serde_json::json!({
                        "line": format!("WARNING: could not create artifact dir, steps will use stdout output: {error}")
                    }),
                );
                None
            }
        }
    });
    crate::pipeline::api_common::reset_write_budget();
    (writer, artifact_dir)
}

/// Begin mirroring logs before extraction has produced a paper hash/run
/// directory. Failed extractions retain this transcript under
/// ~/.pipeline/logs/preprocessing; successful runs adopt it as run.log.
pub(super) fn start_preprocessing_log() -> Option<std::path::PathBuf> {
    let root = dirs::home_dir()?
        .join(".pipeline")
        .join("logs")
        .join("preprocessing");
    std::fs::create_dir_all(&root).ok()?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f");
    let path = root.join(format!("extract-{stamp}-{}.log", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .ok()?;
    crate::pipeline::logging::set_log_sink(Some(file));

    // Retain a bounded set of failed/pre-run transcripts. Successful ones are
    // moved into their run directory below.
    if let Ok(entries) = std::fs::read_dir(&root) {
        let mut files: Vec<_> = entries
            .flatten()
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata.is_file().then_some((
                    metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                    entry.path(),
                ))
            })
            .collect();
        files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        for (_, stale) in files.into_iter().skip(20) {
            let _ = std::fs::remove_file(stale);
        }
    }
    Some(path)
}

pub(super) fn write_run_input_file(
    root: &std::path::Path,
    prefix: &str,
    suffix: &str,
    content: &str,
    label: &str,
) -> Result<(tempfile::NamedTempFile, String), String> {
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(suffix)
        .tempfile_in(root)
        .map_err(|error| format!("Failed to create {label} temp file: {error}"))?;
    let clean_content = crate::safety::strip_span_tags(content);
    file.write_all(clean_content.as_bytes())
        .map_err(|error| format!("Failed to write {label} temp file: {error}"))?;
    file.flush()
        .map_err(|error| format!("Failed to flush {label} temp file: {error}"))?;
    let path = crate::pipeline::claude::normalize_cli_root(&file.path().to_string_lossy())
        .ok_or_else(|| format!("Failed to resolve {label} temp file"))?;
    Ok((file, path))
}

/// Write a completed run's artifacts and manifest into the run directory:
/// numbered per-step files, `report.md`, `report.json` (structured, for resume),
/// any model-written files, the console log, and the manifest. Returns the run
/// id on success. Shared by fresh runs and re-runs.
pub(super) fn finalize_run(
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
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let header = format!(
            "# {}\n\n**Phase**: {} · **Agent**: {}\n\n---\n\n",
            output.step_label,
            output.phase,
            if output.agent.is_empty() {
                "default"
            } else {
                &output.agent
            },
        );
        let rel = format!("artifacts/{:02}_{}.md", i + 1, slug);
        if let Err(e) = w.add_text(
            &rel,
            &output.step_label,
            "step",
            &format!("{}{}", header, output.raw_text),
        ) {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: {e}") }),
            );
        }
    }
    if let Err(e) = w.add_text("report.md", "Report", "report", markdown) {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!("WARNING: {e}") }),
        );
    }
    // Structured report, so a re-run can reload prior step outputs.
    let report_json_durable = serde_json::to_string_pretty(report)
        .map_err(|error| format!("Failed to serialize report data: {error}"))
        .and_then(|json| w.add_text("report.json", "Report data", "context", &json))
        .map_err(|error| {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!("WARNING: {error}") }),
            );
        })
        .is_ok();
    // Structured checkpoints are only needed until report.json is durable.
    // Keep the final artifact tree uncluttered; interrupted runs retain their
    // checkpoint directory for recovery and inspection.
    if report_json_durable {
        let _ = std::fs::remove_dir_all(w.dir().join("artifacts").join("checkpoints"));
    }
    let preserved_responses = w.register_unlisted("artifacts/agent-responses", "agent_response");
    if preserved_responses > 0 {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!(
                    "Registered {preserved_responses} preserved agent response(s)"
                )
            }),
        );
    }
    let extra_files = w.register_unlisted("artifacts", "files");
    if extra_files > 0 {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!("Registered {extra_files} model-written supporting files")
            }),
        );
    }
    match w.compact_page_artifacts() {
        Ok(count) if count > 0 => {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("Compacted {count} page records into one lazy artifact index")
                }),
            );
        }
        Ok(_) => {}
        Err(error) => {
            // Keep the ordinary per-page manifest entries as a compatible
            // fallback if an imported or legacy filename is irregular.
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: could not compact page artifact index: {error}")
                }),
            );
        }
    }
    // Close the console transcript before registering it so the file is complete.
    crate::pipeline::logging::set_log_sink(None);
    let _ = w.register_existing("logs/run.log", "Console log", "context");

    match w.finish(meta) {
        Ok(manifest) => Some(manifest.run_id),
        Err(e) => {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("WARNING: could not write run manifest: {e}")
                }),
            );
            None
        }
    }
}

/// Purge old runs beyond the retention cap (0 = keep all), logging how many.
pub(super) fn enforce_retention(app: &crate::emit::EventBus, keep: usize, max_bytes: u64) {
    if keep == 0 && max_bytes == 0 {
        return;
    }
    if let Ok(n) = crate::runs::purge_runs_with_limits(keep, max_bytes) {
        if n > 0 {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("Removed {n} old run(s) to satisfy history retention limits")
                }),
            );
        }
    }
}
