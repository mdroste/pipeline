//! Every model a review will call, worked out with the executor's own rules.
//!
//! The New review screen shows this list so a researcher can see, before the
//! run, which calls follow their review model and which do not (a Settings
//! stage exception, a workflow-pinned step, the usage-limit backup). The
//! frontend renders it; it never re-derives provider or model inheritance.

use crate::pipeline_config::{Phase, PipelineConfig};
use crate::settings::{ModelSelection, Settings};

/// One group of calls that share a provider, model, and thinking level.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RosterEntry {
    /// `extraction`, `orientation`, `parallel`, `merge`, `sequential`,
    /// `reconcile`, or `fallback`.
    pub stage: &'static str,
    pub label: String,
    pub step_ids: Vec<String>,
    pub step_labels: Vec<String>,
    pub provider: String,
    pub transport: String,
    /// The selection the call resolves. `automatic` means the CLI (or, on an
    /// API connection, Pipeline's recommended model) picks.
    pub model: ModelSelection,
    /// Empty when the provider's default thinking level applies.
    pub effort: String,
    /// `review` (follows this review's model), `default` (follows the
    /// default model in Settings), `settings` (a Settings stage exception),
    /// or `workflow` (pinned by the workflow).
    pub source: &'static str,
    /// When the call may not run, for example "Only after a usage limit".
    pub note: Option<String>,
}

/// What a call resolves before the dispatcher applies the shared default.
fn effective(
    settings: &Settings,
    provider: &str,
    selection: Option<ModelSelection>,
) -> ModelSelection {
    selection.unwrap_or_else(|| settings.model_selection(provider))
}

struct Baseline {
    provider: String,
    model: ModelSelection,
    effort: String,
    source: &'static str,
}

impl Baseline {
    fn source(
        &self,
        provider: &str,
        model: &ModelSelection,
        effort: &str,
        pinned: bool,
    ) -> &'static str {
        if provider == self.provider && *model == self.model && effort == self.effort {
            self.source
        } else if pinned {
            "workflow"
        } else {
            "settings"
        }
    }
}

/// Build the roster for a run whose overrides were already applied to
/// `settings` and `config`. `raw_config` is the workflow before any
/// override or Settings default, so its step agents are workflow pins.
pub fn model_roster(
    settings: &Settings,
    config: &PipelineConfig,
    raw_config: &PipelineConfig,
    review_chosen: bool,
    parallel_chosen: bool,
    diff: bool,
) -> Vec<RosterEntry> {
    let preferred = settings.preferred_provider.as_str();
    let baseline = Baseline {
        provider: preferred.to_string(),
        model: settings.model_selection(preferred),
        effort: settings.model_effort(preferred).to_string(),
        source: if review_chosen { "review" } else { "default" },
    };
    let mut entries: Vec<RosterEntry> = Vec::new();
    let mut push = |entry: RosterEntry| {
        // Neighbouring steps on the same model read as one line.
        if let Some(last) = entries.last_mut() {
            if last.stage == entry.stage
                && last.provider == entry.provider
                && last.model == entry.model
                && last.effort == entry.effort
                && last.source == entry.source
                && last.note == entry.note
                && !entry.step_ids.is_empty()
            {
                last.step_ids.extend(entry.step_ids);
                last.step_labels.extend(entry.step_labels);
                return;
            }
        }
        entries.push(entry);
    };
    let transport = |provider: &str| settings.model_transport(provider).to_string();

    let extractor = {
        let method = config.extraction.method.trim();
        let configured = if method.is_empty() || method == "auto" {
            settings.pdf_extractor.as_str()
        } else {
            method
        };
        crate::settings::resolve_pdf_extractor(configured)
    };
    let input_mode = config.extraction.input_mode.trim();
    if extractor == "llm" && !matches!(input_mode, "none" | "folder") {
        let model = settings.model_selection(preferred);
        let effort = settings.model_effort(preferred).to_string();
        push(RosterEntry {
            stage: "extraction",
            label: "Input processing".into(),
            step_ids: Vec::new(),
            step_labels: Vec::new(),
            provider: preferred.to_string(),
            transport: transport(preferred),
            source: baseline.source(preferred, &model, &effort, false),
            model,
            effort,
            note: Some("Only for PDF input".into()),
        });
    }

    let orientation = settings.orientation_agent().to_string();
    let model = effective(
        settings,
        &orientation,
        settings.orientation_model_selection(&orientation),
    );
    let effort = settings.orientation_effort(&orientation);
    push(RosterEntry {
        stage: "orientation",
        label: "Orientation map".into(),
        step_ids: Vec::new(),
        step_labels: Vec::new(),
        transport: transport(&orientation),
        source: baseline.source(&orientation, &model, &effort, false),
        provider: orientation,
        model,
        effort,
        note: None,
    });

    let mut needs_merge = false;
    for phase in [Phase::Parallel, Phase::Sequential] {
        for step in config
            .steps
            .iter()
            .filter(|step| step.enabled && step.kind.is_model() && step.phase == phase)
        {
            let pinned = raw_config
                .steps
                .iter()
                .find(|raw| raw.id == step.id)
                .is_some_and(|raw| !raw.agents.is_empty());
            needs_merge |= phase == Phase::Parallel && step.agents.len() > 1;
            for provider in &step.agents {
                let model = effective(
                    settings,
                    provider,
                    step.model_selection_for(settings, provider),
                );
                let effort = step.effort_for(settings, provider);
                let mut source = baseline.source(provider, &model, &effort, pinned);
                if source == "settings" && parallel_chosen && phase == Phase::Parallel && !pinned {
                    source = "review";
                }
                push(RosterEntry {
                    stage: if phase == Phase::Parallel {
                        "parallel"
                    } else {
                        "sequential"
                    },
                    label: if phase == Phase::Parallel {
                        "Parallel steps".into()
                    } else {
                        "Sequential steps".into()
                    },
                    step_ids: vec![step.id.clone()],
                    step_labels: vec![if step.label.trim().is_empty() {
                        step.id.clone()
                    } else {
                        step.label.clone()
                    }],
                    transport: transport(provider),
                    provider: provider.clone(),
                    model,
                    effort,
                    source,
                    note: None,
                });
            }
        }
        if phase == Phase::Parallel && config.merge.enabled && needs_merge {
            // Mirrors pipeline::merge: a workflow Merge agent runs on its
            // provider's default; an inherited one takes the Merge policy.
            let pinned = config.merge.agents.first().cloned();
            let provider = pinned
                .clone()
                .unwrap_or_else(|| settings.merge_agent().to_string());
            let (model, effort) = if pinned.is_some() {
                (
                    settings.model_selection(&provider),
                    settings.model_effort(&provider).to_string(),
                )
            } else {
                (
                    effective(
                        settings,
                        &provider,
                        settings.merge_model_selection(&provider),
                    ),
                    settings.merge_effort(&provider),
                )
            };
            push(RosterEntry {
                stage: "merge",
                label: "Combine results".into(),
                step_ids: Vec::new(),
                step_labels: Vec::new(),
                transport: transport(&provider),
                source: baseline.source(&provider, &model, &effort, pinned.is_some()),
                provider,
                model,
                effort,
                note: None,
            });
        }
    }

    if diff || settings.auto_revision_reconciliation {
        let model = settings.model_selection(preferred);
        let effort = settings.model_effort(preferred).to_string();
        push(RosterEntry {
            stage: "reconcile",
            label: "Revision comparison".into(),
            step_ids: Vec::new(),
            step_labels: Vec::new(),
            provider: preferred.to_string(),
            transport: transport(preferred),
            source: baseline.source(preferred, &model, &effort, false),
            model,
            effort,
            note: (!diff).then(|| "Only when a previous report exists".into()),
        });
    }

    if let Some(fallback) = settings.usage_limit_fallback_agent() {
        let model = effective(
            settings,
            fallback,
            settings.usage_limit_fallback_model_selection(fallback),
        );
        push(RosterEntry {
            stage: "fallback",
            label: "Usage-limit backup".into(),
            step_ids: Vec::new(),
            step_labels: Vec::new(),
            provider: fallback.to_string(),
            transport: transport(fallback),
            model,
            effort: settings.usage_limit_fallback_effort(fallback),
            source: "settings",
            note: Some("Only after a usage limit".into()),
        });
    }
    entries
}

#[cfg(test)]
mod tests;
