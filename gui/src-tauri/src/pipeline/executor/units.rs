//! Deterministic file/artifact fan-out and agent dispatch units.
use super::findings::{normalized_finding_hash, PreservedFinding};
use super::outputs::effective_output_schema;
use super::paths::step_slug;
use crate::output::capitalize;
use crate::pipeline_config::StepConfig;

/// One dispatch of a parallel step: either an agent (normal / multi-agent) or a
/// fan-out item (a file, with `{item}` bound). `suffix` is the composite-key
/// suffix ("" for a single plain run).
pub(super) struct Unit {
    pub(super) agent: String,
    /// File fan-out: the matched path, staged into the unit's artifact view.
    pub(super) item: Option<String>,
    /// Artifact fan-out: the element text bound directly to `{item}`.
    pub(super) inline_item: Option<String>,
    /// Ordered finding ids this unit's response must preserve (artifact
    /// fan-out over a findings product with the preserve-findings marker).
    pub(super) lineage_ids: Option<Vec<PreservedFinding>>,
    pub(super) suffix: String,
    pub(super) display: String,
    /// Suffix shared by all agents analyzing the same logical item. Empty for
    /// ordinary non-fan-out work.
    pub(super) item_suffix: String,
    pub(super) merge_agents: bool,
}

/// The units a step dispatches, and what its fan-out limit left out.
#[derive(Default)]
pub(super) struct UnitPlan {
    pub(super) units: Vec<Unit>,
    pub(super) coverage_note: Option<String>,
}

/// The report's statement of a fan-out that stopped at its limit. The run still
/// succeeds, so this is the only sign that part of the input went unread.
fn coverage_note(label: &str, ran: usize, total: usize, noun: &str) -> Option<String> {
    let left = total.checked_sub(ran).filter(|left| *left > 0)?;
    let unread = if left == 1 {
        "1 was not read".to_string()
    } else {
        format!("{left} were not read")
    };
    Some(format!(
        "Step '{label}' ran on {ran} of {total} {noun}. It stops at {ran}, so {unread}."
    ))
}

/// The directory a fan-out glob is resolved against: the input folder itself,
/// or the parent directory of a single-file input.
pub(super) fn fan_out_root(source_path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(source_path);
    if p.is_dir() {
        p.to_path_buf()
    } else {
        p.parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    }
}

/// Build the units to run for a step. Fan-out and agent selection are
/// independent dimensions, so a multi-agent fan-out produces their Cartesian
/// product. Fan-out with no matches returns empty; incomplete discovery is an
/// error rather than being misreported as a valid zero-match scan.
pub(super) fn build_units(
    step: &StepConfig,
    settings: &crate::settings::Settings,
    source_path: &str,
    artifact_source: &[String],
    app: &crate::emit::EventBus,
) -> Result<UnitPlan, String> {
    let agents: Vec<String> = if step.agents.is_empty() {
        vec![settings.preferred_provider.clone()]
    } else {
        step.agents.clone()
    };
    let multi = agents.len() > 1;

    let mut note = None;
    let units = if let Some(fe) = &step.for_each {
        if let Some(source) = &fe.artifact {
            return build_artifact_units(step, source, fe, artifact_source, agents, multi, app);
        }
        // No-input profiles and runs whose source staging failed have an
        // empty source root; fan_out_root("") would resolve to the process
        // cwd, so treat this as zero matches instead of scanning it.
        if source_path.is_empty() {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "WARNING: fan-out step '{}' has no source root to scan; treating glob '{}' as matching no files", step.label, fe.glob
                )}),
            );
            return Ok(UnitPlan::default());
        }
        let root = fan_out_root(source_path);
        let expansion = crate::pipeline::glob::expand(&root, &fe.glob, fe.max.max(1) as usize);
        let total_matches = expansion.total_matches;
        let items = expansion.matches;
        if let Some(reason) = expansion.limited_by {
            if reason != "match limit" {
                return Err(if reason == "cancelled" {
                    "Pipeline cancelled during fan-out discovery".to_string()
                } else {
                    format!(
                        "Fan-out discovery for '{}' stopped at the {reason}; results may be incomplete",
                        step.label
                    )
                });
            }
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "Fan-out step '{}' was bounded by {reason} at {} files (glob '{}')", step.label, items.len(), fe.glob
                )}),
            );
            note = coverage_note(&step.label, items.len(), total_matches, "matching files");
        }
        if items.is_empty() {
            let _ = app.emit_event(
                "pipeline:log",
                serde_json::json!({ "line": format!(
                    "WARNING: fan-out step '{}' matched no files for glob '{}'", step.label, fe.glob
                )}),
            );
            return Ok(UnitPlan::default());
        }
        let mut used = std::collections::HashSet::new();
        items
            .into_iter()
            .flat_map(|path| {
                let base = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone());
                let mut suffix = step_slug(&base);
                let mut n = 2;
                while !used.insert(suffix.clone()) {
                    suffix = format!("{}_{}", step_slug(&base), n);
                    n += 1;
                }
                agents.iter().cloned().map(move |agent| Unit {
                    suffix: if multi {
                        format!("{suffix}/{agent}")
                    } else {
                        suffix.clone()
                    },
                    item_suffix: suffix.clone(),
                    display: base.clone(),
                    agent,
                    item: Some(path.clone()),
                    inline_item: None,
                    lineage_ids: None,
                    merge_agents: multi,
                })
            })
            .collect::<Vec<_>>()
    } else {
        agents
            .into_iter()
            .map(|a| Unit {
                suffix: if multi { a.clone() } else { String::new() },
                display: capitalize(&a),
                agent: a,
                item: None,
                inline_item: None,
                lineage_ids: None,
                item_suffix: String::new(),
                merge_agents: multi,
            })
            .collect::<Vec<_>>()
    };
    Ok(UnitPlan {
        units,
        coverage_note: note,
    })
}

/// Deterministic composite-key suffix for the Nth artifact fan-out element.
/// Indexes rather than element content key the units, so reassembly can align
/// unit outputs with their source elements even when elements collide.
pub(super) fn artifact_item_suffix(index: usize) -> String {
    format!("item_{:03}", index + 1)
}

/// Expand an artifact fan-out into one unit per upstream array element.
pub(super) fn build_artifact_units(
    step: &StepConfig,
    source: &crate::pipeline_config::ForEachArtifact,
    fe: &crate::pipeline_config::ForEach,
    artifact_source: &[String],
    agents: Vec<String>,
    multi: bool,
    app: &crate::emit::EventBus,
) -> Result<UnitPlan, String> {
    let (elements, total) = artifact_fan_out_elements(step, source, fe, artifact_source, app)?;
    // Per-element lineage applies when this step's own contract preserves the
    // findings of exactly the step it fans out over.
    let preserves_source = effective_output_schema(step)?
        .as_ref()
        .and_then(|schema| schema.get(crate::pipeline::structured::PRESERVE_FINDINGS_KEY))
        .and_then(serde_json::Value::as_str)
        == Some(source.step.as_str());
    let mut units = Vec::new();
    for (index, element) in elements.iter().enumerate() {
        let element_id = element
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty());
        let display = element_id.map_or_else(|| format!("item {}", index + 1), str::to_string);
        let item_text = match element {
            serde_json::Value::String(text) => text.clone(),
            other => serde_json::to_string_pretty(other).map_err(|error| {
                format!("Fan-out element {index} cannot be serialized: {error}")
            })?,
        };
        let lineage_ids = if preserves_source {
            match element_id {
                Some(id) => Some(vec![PreservedFinding {
                    id: id.to_string(),
                    before_hash: normalized_finding_hash(element)?,
                }]),
                None => None,
            }
        } else {
            None
        };
        let suffix = artifact_item_suffix(index);
        for agent in &agents {
            units.push(Unit {
                suffix: if multi {
                    format!("{suffix}/{agent}")
                } else {
                    suffix.clone()
                },
                item_suffix: suffix.clone(),
                display: display.clone(),
                agent: agent.clone(),
                item: None,
                inline_item: Some(item_text.clone()),
                lineage_ids: lineage_ids.clone(),
                merge_agents: multi,
            });
        }
    }
    Ok(UnitPlan {
        units,
        coverage_note: coverage_note(
            &step.label,
            elements.len(),
            total,
            &format!("items from '{}'", source.step),
        ),
    })
}

pub(super) fn source_artifacts(outputs: &[crate::models::StepOutput], step: &str) -> Vec<String> {
    outputs
        .iter()
        .filter(|out| !out.skipped && out.step_id.split('/').next() == Some(step))
        .map(|out| out.raw_text.clone())
        .collect()
}

/// The bounded dispatch list and the full count of the source list. Reassembly
/// must instead read the complete source list.
pub(super) fn artifact_fan_out_elements(
    step: &StepConfig,
    source: &crate::pipeline_config::ForEachArtifact,
    fe: &crate::pipeline_config::ForEach,
    artifact_source: &[String],
    app: &crate::emit::EventBus,
) -> Result<(Vec<serde_json::Value>, usize), String> {
    if artifact_source.is_empty() {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "WARNING: fan-out step '{}' has no artifact from step '{}'; treating it as zero items",
                step.label, source.step
            )}),
        );
        return Ok((Vec::new(), 0));
    };
    let max = fe.max.max(1) as usize;
    let mut picked = Vec::new();
    let mut total = 0;
    for text in artifact_source {
        let value = serde_json::from_str::<serde_json::Value>(text.trim()).map_err(|error| {
            format!(
                "Fan-out step '{}' needs structured JSON from step '{}': {error}",
                step.label, source.step
            )
        })?;
        let target = if source.pointer.is_empty() {
            &value
        } else {
            value.pointer(&source.pointer).ok_or_else(|| {
                format!(
                    "Fan-out step '{}' found no value at '{}' in the artifact from step '{}'",
                    step.label, source.pointer, source.step
                )
            })?
        };
        let elements = target.as_array().ok_or_else(|| {
            format!(
                "Fan-out step '{}' expects an array at '{}' in the artifact from step '{}'",
                step.label, source.pointer, source.step
            )
        })?;
        total += elements.len();
        picked.extend(
            elements
                .iter()
                .take(max.saturating_sub(picked.len()))
                .cloned(),
        );
    }
    if total > max {
        let _ = app.emit_event(
            "pipeline:log",
            serde_json::json!({ "line": format!(
                "Fan-out step '{}' was bounded by its item cap at {max} of {} elements",
                step.label,
                total
            )}),
        );
    }
    Ok((picked, total))
}
