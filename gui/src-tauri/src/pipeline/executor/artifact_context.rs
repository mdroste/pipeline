//! Private selector-filtered artifact views and shared context preparation.
use super::paths::step_slug;
use super::schedule::base_id;
use crate::models::StepOutput;
use crate::pipeline::claude::normalize_cli_root;
use crate::pipeline_config::{
    ArtifactSelector, NamedInputArtifactPart, Phase, PrimaryArtifactPart, StepArtifactPart,
    StepConfig,
};
use std::sync::Arc;

/// Runtime inventory from which a step's private, selector-filtered artifact
/// view is constructed.
pub(super) struct ArtifactRuntime<'a> {
    pub(super) orientation_path: &'a str,
    pub(super) paper_text_path: &'a str,
    pub(super) document_bundle_path: &'a str,
    pub(super) source_path: &'a str,
    pub(super) extra_inputs: &'a std::collections::HashMap<String, String>,
    pub(super) extra_input_sources: &'a std::collections::HashMap<String, String>,
    pub(super) outputs: &'a [StepOutput],
    pub(super) run_artifact_dir: Option<&'a str>,
}

/// Concrete, call-owned view of a step's selected artifacts. App-controlled
/// files are staged into one private root so directory-scoped CLI permissions
/// cannot expose unselected siblings from the run temp directory.
pub(super) struct ResolvedArtifactContext {
    pub(super) _view: tempfile::TempDir,
    pub(super) orientation_path: String,
    pub(super) paper_text_path: String,
    pub(super) document_bundle_path: String,
    pub(super) source_path: String,
    pub(super) extra_inputs: std::collections::HashMap<String, String>,
    pub(super) prior_outputs: Vec<StepOutput>,
    pub(super) read_dirs: Vec<String>,
    pub(super) artifact_root: Option<String>,
    pub(super) manifest: String,
    pub(super) has_visuals: bool,
    pub(super) includes_primary_text: bool,
    pub(super) includes_survey: bool,
}

impl ResolvedArtifactContext {
    pub(super) fn stage_current_item(&mut self, source: &str) -> Result<String, String> {
        let name = std::path::Path::new(source)
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("item"));
        let staged = stage_artifact_file(
            self._view.path(),
            source,
            &std::path::Path::new("input/current-item").join(name),
        )?;
        self.manifest
            .push_str(&format!("\n- Current fan-out item: {staged}"));
        Ok(staged)
    }
}

pub(super) fn normalized_path(path: &std::path::Path) -> String {
    normalize_cli_root(&path.to_string_lossy())
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
}

pub(super) fn stage_artifact_file(
    view: &std::path::Path,
    source: &str,
    relative: &std::path::Path,
) -> Result<String, String> {
    let source_path = std::path::Path::new(source);
    let destination = view.join(relative);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create artifact view: {error}"))?;
    }
    if std::fs::hard_link(source_path, &destination).is_err() {
        std::fs::copy(source_path, &destination).map_err(|error| {
            format!(
                "Failed to stage selected artifact '{}': {error}",
                source_path.display()
            )
        })?;
    }
    Ok(normalized_path(&destination))
}

pub(super) fn stage_document_index(
    root: &std::path::Path,
    source: &str,
    relative: &std::path::Path,
) -> Result<String, String> {
    let raw = std::fs::read_to_string(source)
        .map_err(|error| format!("Failed to read DocumentBundle for model index: {error}"))?;
    let bundle: crate::document_bundle::DocumentBundle = serde_json::from_str(&raw)
        .map_err(|error| format!("Failed to parse DocumentBundle for model index: {error}"))?;
    bundle.validate()?;
    let index = bundle.to_model_index_pretty()?;
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create document-index directory: {error}"))?;
    }
    std::fs::write(&destination, index)
        .map_err(|error| format!("Failed to stage document index: {error}"))?;
    Ok(normalized_path(&destination))
}

pub(super) fn add_read_root(read_dirs: &mut Vec<String>, path: &std::path::Path) {
    if path.exists() {
        read_dirs.push(normalized_path(path));
    }
}

pub(super) fn selected_primary_parts(
    step: &StepConfig,
) -> std::collections::HashSet<PrimaryArtifactPart> {
    step.context
        .include
        .iter()
        .find_map(|selector| match selector {
            ArtifactSelector::Primary { parts } => Some(parts.iter().copied().collect()),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn resolve_artifact_context(
    step: &StepConfig,
    runtime: ArtifactRuntime<'_>,
) -> Result<ResolvedArtifactContext, String> {
    if step.phase == Phase::Parallel
        && step
            .context
            .include
            .iter()
            .any(|selector| matches!(selector, ArtifactSelector::Step { .. }))
    {
        return Err(format!(
            "Parallel step '{}' cannot consume another step's output",
            step.id
        ));
    }
    let view = tempfile::Builder::new()
        .prefix("pipeline_step_context_")
        .tempdir()
        .map_err(|error| format!("Failed to create context view for '{}': {error}", step.id))?;
    let mut read_dirs = Vec::new();
    let mut manifest_lines = vec![format!("ARTIFACT CONTEXT FOR STEP '{}':", step.label)];
    let primary_parts = selected_primary_parts(step);

    let paper_text_path = if primary_parts.contains(&PrimaryArtifactPart::Text)
        && !runtime.paper_text_path.is_empty()
    {
        let path = stage_artifact_file(
            view.path(),
            runtime.paper_text_path,
            std::path::Path::new("input/main/document.md"),
        )?;
        manifest_lines.push(format!("- Primary readable document: {path}"));
        path
    } else {
        String::new()
    };

    let document_bundle_path = if primary_parts.contains(&PrimaryArtifactPart::Structure)
        && !runtime.document_bundle_path.is_empty()
    {
        let path = stage_document_index(
            view.path(),
            runtime.document_bundle_path,
            std::path::Path::new("input/main/document_index.json"),
        )?;
        manifest_lines.push(format!(
            "- Compact primary document structure index: {path}"
        ));
        path
    } else {
        String::new()
    };

    let includes_survey = step
        .context
        .include
        .iter()
        .any(|selector| matches!(selector, ArtifactSelector::Survey));
    let orientation_path = if includes_survey && !runtime.orientation_path.is_empty() {
        let path = stage_artifact_file(
            view.path(),
            runtime.orientation_path,
            std::path::Path::new("survey/orientation.json"),
        )?;
        manifest_lines.push(format!("- Survey: {path}"));
        path
    } else {
        String::new()
    };

    let source_path = if primary_parts.contains(&PrimaryArtifactPart::Source)
        && !runtime.source_path.is_empty()
    {
        let source = std::path::Path::new(runtime.source_path);
        if source.is_dir() {
            add_read_root(&mut read_dirs, source);
            let path = normalized_path(source);
            manifest_lines.push(format!("- Primary source tree: {path}"));
            path
        } else {
            let name = source
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("source"));
            let path = stage_artifact_file(
                view.path(),
                runtime.source_path,
                &std::path::Path::new("input/main/source").join(name),
            )?;
            manifest_lines.push(format!("- Primary source file: {path}"));
            path
        }
    } else {
        String::new()
    };

    let has_visuals = primary_parts.contains(&PrimaryArtifactPart::Visuals);
    let artifact_root = if has_visuals {
        runtime
            .run_artifact_dir
            .and_then(|directory| std::path::Path::new(directory).parent())
            .map(normalized_path)
    } else {
        None
    };
    if has_visuals {
        if let Some(root) = runtime.run_artifact_dir {
            let root = std::path::Path::new(root);
            for subdir in ["pages", "figures", "document"] {
                add_read_root(&mut read_dirs, &root.join(subdir));
            }
        }
        if let Some(root) = artifact_root.as_deref() {
            manifest_lines.push(format!(
                "- Primary visual assets (bundle-relative root): {root}"
            ));
        }
    }

    let mut extra_inputs = std::collections::HashMap::new();
    for selector in &step.context.include {
        let ArtifactSelector::NamedInput { key, parts } = selector else {
            continue;
        };
        if parts.contains(&NamedInputArtifactPart::Text) {
            if let Some(source) = runtime.extra_inputs.get(key) {
                let path = stage_artifact_file(
                    view.path(),
                    source,
                    &std::path::Path::new("input/named").join(format!("{key}.md")),
                )?;
                manifest_lines.push(format!("- Named input '{key}' text: {path}"));
                extra_inputs.insert(key.clone(), path);
            }
        }
        if parts.contains(&NamedInputArtifactPart::Source) {
            if let Some(source) = runtime.extra_input_sources.get(key) {
                let original = std::path::Path::new(source);
                if original.is_dir() {
                    add_read_root(&mut read_dirs, original);
                    manifest_lines.push(format!(
                        "- Named input '{key}' source tree: {}",
                        normalized_path(original)
                    ));
                } else {
                    let name = original
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("source"));
                    let staged = stage_artifact_file(
                        view.path(),
                        source,
                        &std::path::Path::new("input/named")
                            .join(key)
                            .join("source")
                            .join(name),
                    )?;
                    manifest_lines.push(format!("- Named input '{key}' source file: {staged}"));
                }
            }
        }
    }

    let report_steps: std::collections::HashSet<&str> = step
        .context
        .include
        .iter()
        .filter_map(|selector| match selector {
            ArtifactSelector::Step { step, parts, .. }
                if parts.contains(&StepArtifactPart::Report) =>
            {
                Some(step.as_str())
            }
            _ => None,
        })
        .collect();
    let prior_outputs: Vec<StepOutput> = runtime
        .outputs
        .iter()
        .filter(|output| report_steps.contains(base_id(&output.step_id)))
        .cloned()
        .collect();
    for output in &prior_outputs {
        let relative = std::path::Path::new("steps")
            .join(step_slug(base_id(&output.step_id)))
            .join(step_slug(&output.step_id))
            .join("report.md");
        let destination = view.path().join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to stage upstream report: {error}"))?;
        }
        std::fs::write(&destination, &output.raw_text)
            .map_err(|error| format!("Failed to stage upstream report: {error}"))?;
        manifest_lines.push(format!(
            "- Upstream report '{}' [{}]: {}",
            output.step_label,
            output.step_id,
            normalized_path(&destination)
        ));
    }

    if let Some(run_root) = runtime.run_artifact_dir {
        for selector in &step.context.include {
            let ArtifactSelector::Step {
                step: producer,
                parts,
                glob,
            } = selector
            else {
                continue;
            };
            if !parts.contains(&StepArtifactPart::Files) {
                continue;
            }
            let producer_root = std::path::Path::new(run_root)
                .join("by-step")
                .join(step_slug(producer));
            let Ok(instances) = std::fs::read_dir(&producer_root) else {
                continue;
            };
            let mut unit_files: Vec<(String, std::path::PathBuf)> = instances
                .flatten()
                .filter_map(|instance| {
                    let files = instance.path().join("files");
                    files
                        .is_dir()
                        .then(|| (instance.file_name().to_string_lossy().into_owned(), files))
                })
                .collect();
            unit_files.sort();
            // A multi-agent or fan-out producer has several units; staging
            // their trees into one shared destination would let same-named
            // files overwrite each other in read_dir order. Namespace each
            // unit by its on-disk unit slug (the report-staging convention);
            // a single unit keeps the flat layout.
            let namespace_units = unit_files.len() > 1;
            for (unit, files) in &unit_files {
                if glob.is_empty() {
                    add_read_root(&mut read_dirs, files);
                    manifest_lines.push(format!(
                        "- Supporting files from '{producer}': {}",
                        normalized_path(files)
                    ));
                } else {
                    let expanded = crate::pipeline::glob::expand(files, glob, 500);
                    if expanded
                        .limited_by
                        .is_some_and(|reason| reason != "match limit")
                    {
                        return Err(format!(
                            "Supporting-file selection for '{}' was incomplete ({})",
                            producer,
                            expanded.limited_by.unwrap_or("unknown limit")
                        ));
                    }
                    for matched in expanded.matches {
                        let source = std::path::Path::new(&matched);
                        let relative = source
                            .strip_prefix(files)
                            .map_err(|_| "Selected supporting file escaped its producer root")?;
                        let mut destination =
                            std::path::Path::new("steps").join(step_slug(producer));
                        if namespace_units {
                            destination = destination.join(unit);
                        }
                        let staged = stage_artifact_file(
                            view.path(),
                            &matched,
                            &destination.join("files").join(relative),
                        )?;
                        manifest_lines
                            .push(format!("- Supporting file from '{producer}': {staged}"));
                    }
                }
            }
        }
    }

    // Every staged file is beneath this one root. Original source trees and
    // unfiltered producer file roots were added separately above.
    add_read_root(&mut read_dirs, view.path());
    read_dirs.sort();
    read_dirs.dedup();

    Ok(ResolvedArtifactContext {
        _view: view,
        orientation_path,
        paper_text_path,
        document_bundle_path,
        source_path,
        extra_inputs,
        prior_outputs,
        read_dirs,
        artifact_root,
        manifest: manifest_lines.join("\n"),
        has_visuals,
        includes_primary_text: primary_parts.contains(&PrimaryArtifactPart::Text),
        includes_survey,
    })
}

/// Shared-context reuse is an optimization, never a precondition: any
/// failure to assemble the shared prefix (unreadable staged text, or content
/// past the runtime-context safety cap — possible because LaTeX extraction
/// permits more than the 8 MB shared-context limit) degrades the affected
/// calls to ordinary self-contained prompts instead of failing the run.
pub(super) fn prepare_selected_shared_context(
    app: &crate::emit::EventBus,
    enabled: bool,
    resolved: &ResolvedArtifactContext,
    orientation: &serde_json::Value,
    pool: &crate::pipeline::context_cache::PreparedContextPool,
) -> Option<Arc<crate::pipeline::context_cache::PreparedContext>> {
    if !enabled || (!resolved.includes_primary_text && !resolved.includes_survey) {
        return None;
    }
    let degrade = |error: &str| {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({
                "line": format!(
                    "WARNING: shared input context unavailable ({error}); running without context reuse"
                )
            }),
        );
        None
    };
    let text = if resolved.includes_primary_text && !resolved.paper_text_path.is_empty() {
        match std::fs::read_to_string(&resolved.paper_text_path) {
            Ok(text) => text,
            Err(error) => return degrade(&error.to_string()),
        }
    } else {
        String::new()
    };
    let empty_survey = serde_json::Value::Null;
    let survey = if resolved.includes_survey {
        orientation
    } else {
        &empty_survey
    };
    match pool.prepare(&text, survey) {
        Ok(prepared) => Some(prepared),
        Err(error) => degrade(&error),
    }
}
