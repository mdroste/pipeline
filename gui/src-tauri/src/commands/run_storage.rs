use super::*;

const MIN_RUN_HEADROOM_BYTES: u64 = 512 * 1024 * 1024;

/// Verify that the durable run store is writable before extraction or model
/// work begins. A source-size multiplier covers temporary extraction assets;
/// the fixed floor protects no-input and small-document runs.
pub(super) fn preflight_run_storage(input: &std::path::Path) -> Result<(), String> {
    use std::io::Write as _;

    let root = crate::runs::runs_dir()?;
    let source_bytes = input
        .metadata()
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let required = MIN_RUN_HEADROOM_BYTES.max(source_bytes.saturating_mul(4));
    if let Ok(available) = fs2::available_space(&root) {
        if available < required {
            return Err(format!(
                "Pipeline needs at least {:.1} GB of free space to prepare and durably save this run; {:.1} GB is available.",
                required as f64 / 1_000_000_000.0,
                available as f64 / 1_000_000_000.0,
            ));
        }
    }

    let mut probe = tempfile::NamedTempFile::new_in(&root)
        .map_err(|error| format!("Run history is not writable: {error}"))?;
    probe
        .write_all(b"pipeline-run-storage-probe\n")
        .map_err(|error| format!("Run history is not writable: {error}"))?;
    probe
        .flush()
        .and_then(|()| probe.as_file().sync_all())
        .map_err(|error| format!("Run history cannot be durably synced: {error}"))?;
    Ok(())
}

/// Create the required persistent workspace shared by fresh runs and re-runs.
/// A run never falls back to an unsaved execution path.
pub(super) fn create_run_workspace(
    app: &crate::emit::EventBus,
    paper_hash: &str,
    pending_meta: crate::runs::RunFinishMeta,
    preprocessing_log: Option<&std::path::Path>,
) -> Result<(Option<crate::runs::RunWriter>, Option<String>), String> {
    let mut writer = crate::runs::RunWriter::create_unique(paper_hash)
        .map_err(|error| format!("Could not create the durable run workspace: {error}"))?;
    writer
        .set_pending_meta(pending_meta)
        .map_err(|error| format!("Could not initialize the durable run manifest: {error}"))?;

    {
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

    let dir = writer.dir().join("artifacts");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create the durable artifact directory: {error}"))?;
    let artifact_dir = Some(
        crate::pipeline::claude::normalize_cli_root(&dir.to_string_lossy())
            .unwrap_or_else(|| dir.to_string_lossy().replace('\\', "/")),
    );
    crate::pipeline::api_common::reset_write_budget();
    Ok((Some(writer), artifact_dir))
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
) -> Result<String, String> {
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
        let (extension, contents) = if output.structured_json {
            ("json", output.raw_text.clone())
        } else {
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
            ("md", format!("{}{}", header, output.raw_text))
        };
        let rel = format!("artifacts/{:02}_{}.{}", i + 1, slug, extension);
        w.add_text(&rel, &output.step_label, "step", &contents)
            .map_err(|error| {
                format!(
                    "Could not durably save step '{}': {error}",
                    output.step_label
                )
            })?;
    }
    w.add_text("report.md", "Report", "report", markdown)
        .and_then(|()| crate::runs::sync_run_file(&w.dir().join("report.md")))
        .map_err(|error| format!("Could not durably save report.md: {error}"))?;
    if let Some(findings) = report.products.findings.as_ref() {
        serde_json::to_string_pretty(findings)
            .map_err(|error| format!("Failed to serialize findings: {error}"))
            .and_then(|json| w.add_text("findings.json", "Findings", "product", &json))
            .and_then(|()| crate::runs::sync_run_file(&w.dir().join("findings.json")))
            .map_err(|error| format!("Could not durably save findings.json: {error}"))?;
    }
    for product in &report.products.named {
        let extension = if product.viewer == "json" || !product.content.is_string() {
            "json"
        } else if product.viewer == "markdown" {
            "md"
        } else {
            "txt"
        };
        let contents = if extension == "json" {
            serde_json::to_string_pretty(&product.content).map_err(|error| {
                format!("Failed to serialize product '{}': {error}", product.key)
            })?
        } else {
            product.content.as_str().unwrap_or_default().to_string()
        };
        let rel_path = format!("products/{}.{}", product.key, extension);
        w.add_text(&rel_path, &product.key, "product", &contents)
            .and_then(|()| crate::runs::sync_run_file(&w.dir().join(&rel_path)))
            .map_err(|error| {
                format!(
                    "Could not durably save named product '{}': {error}",
                    product.key
                )
            })?;
    }
    // Structured report, so a re-run can reload prior step outputs. It must be
    // fsynced before the checkpoint directory below is removed.
    serde_json::to_string_pretty(report)
        .map_err(|error| format!("Failed to serialize report data: {error}"))
        .and_then(|json| w.add_text("report.json", "Report data", "context", &json))
        .and_then(|()| crate::runs::sync_run_file(&w.dir().join("report.json")))
        .map_err(|error| format!("Could not durably save report.json: {error}"))?;
    // Structured checkpoints are only needed until report.json is durable.
    // Keep the final artifact tree uncluttered; interrupted runs retain their
    // checkpoint directory for recovery and inspection.
    let checkpoint_dir = w.dir().join("artifacts").join("checkpoints");
    if checkpoint_dir.exists() {
        std::fs::remove_dir_all(&checkpoint_dir)
            .map_err(|error| format!("Could not retire completed step checkpoints: {error}"))?;
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

    w.finish(meta)
        .map(|manifest| manifest.run_id)
        .map_err(|error| format!("Could not publish the final run manifest: {error}"))
}

/// Purge old runs beyond the retention cap (0 = keep all), logging how many.
pub(super) fn enforce_retention(app: &crate::emit::EventBus, keep: usize, max_bytes: u64) {
    if keep == 0 && max_bytes == 0 {
        return;
    }
    match crate::runs::purge_runs_with_limits(keep, max_bytes) {
        Ok(n) if n > 0 => {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({
                    "line": format!("Moved {n} old run(s) to Trash to satisfy history retention limits")
                }),
            );
        }
        Ok(_) => {}
        Err(error) => {
            // A swallowed failure here means the retention caps silently stop
            // being enforced. Surface it everywhere reachable.
            let line = format!("WARNING: run-history retention could not be enforced: {error}");
            eprintln!("{line}");
            crate::pipeline::logging::emit(app, line);
        }
    }
}
