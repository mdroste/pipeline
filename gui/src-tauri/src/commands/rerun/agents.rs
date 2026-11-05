//! A run's one-report agent choice across Resume and Regenerate.
//!
//! The choice is folded into settings and the workflow at launch, after the
//! workflow snapshot is taken, so the run saves it beside that snapshot. A run
//! with no saved file used its Settings and workflow agents, which is also the
//! case for every run from before the file existed. A file that exists but
//! cannot be used stops the re-run: continuing on other agents is the failure
//! this module prevents.

use crate::commands::run_context::{apply_model_overrides, RunModelOverrides};
use crate::pipeline::providers::request_provider_label;
use crate::pipeline_config::PipelineConfig;
use crate::runs::RunWriter;

const AGENT_OVERRIDES_PATH: &str = "context/run_overrides.json";

const UNUSABLE: &str = "This Review was started with its own agent choice, and that choice";

pub(in crate::commands) fn save_agent_overrides(
    writer: &mut RunWriter,
    overrides: Option<&RunModelOverrides>,
) -> Result<(), String> {
    let Some(overrides) = overrides else {
        return Ok(());
    };
    let json = serde_json::to_string_pretty(overrides)
        .map_err(|error| format!("Could not record this Review's agents: {error}"))?;
    writer
        .add_text(AGENT_OVERRIDES_PATH, "One-report agents", "context", &json)
        .map_err(|error| format!("Could not save this Review's agents: {error}"))
}

fn read_agent_overrides(run_dir: &std::path::Path) -> Result<Option<RunModelOverrides>, String> {
    let path = run_dir.join(AGENT_OVERRIDES_PATH);
    if matches!(
        std::fs::symlink_metadata(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ) {
        return Ok(None);
    }
    super::read_run_file_in(run_dir, AGENT_OVERRIDES_PATH)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .map(Some)
        .ok_or_else(|| format!("{UNUSABLE} could not be read. Start a new Review."))
}

pub(super) fn saved_agent_overrides(run_id: &str) -> Result<Option<RunModelOverrides>, String> {
    crate::runs::validate_run_id(run_id)?;
    read_agent_overrides(&crate::runs::runs_dir()?.join(run_id))
}

pub(super) fn reapply(
    overrides: &RunModelOverrides,
    settings: &mut crate::settings::Settings,
    config: &mut PipelineConfig,
) -> Result<(), String> {
    apply_model_overrides(settings, config, overrides)
        .map_err(|error| format!("{UNUSABLE} can no longer be used: {error}. Start a new Review."))
}

fn agent_names<'a>(agents: impl Iterator<Item = &'a str>) -> String {
    agents
        .map(|agent| request_provider_label(agent, "cli"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the researcher sees before continuing, and what the log records.
pub(super) fn notice(overrides: &RunModelOverrides) -> String {
    let mut text =
        "This Review was started with its own agent choice, and continuing uses it again."
            .to_string();
    if let Some(review) = &overrides.review {
        let provider = agent_names(std::iter::once(review.provider.as_str()));
        let model = if review.model.trim().is_empty() {
            format!("{provider}'s choice")
        } else {
            review.model.trim().to_string()
        };
        let effort = if review.effort.trim().is_empty() {
            String::new()
        } else {
            format!(", {} thinking", review.effort.trim())
        };
        text.push_str(&format!(" Review model: {provider}, {model}{effort}."));
    }
    if !overrides.agents.is_empty() {
        let parallel = agent_names(overrides.agents.iter().map(String::as_str));
        text.push_str(&format!(" Parallel: {parallel}."));
    }
    if let Some(merge) = overrides.merge_agent.as_deref() {
        text.push_str(&format!(" Merge: {}.", agent_names(std::iter::once(merge))));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_config::{apply_agent_defaults, Phase, StepConfig};

    fn overrides() -> RunModelOverrides {
        RunModelOverrides {
            agents: vec!["codex".into()],
            merge_agent: Some("codex".into()),
            effort_overrides: [("codex".to_string(), "high".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        }
    }

    fn settings() -> crate::settings::Settings {
        crate::settings::Settings {
            default_parallel_agents: vec!["claude".into()],
            default_merge_agent: "claude".into(),
            ..Default::default()
        }
    }

    fn retained_workflow() -> PipelineConfig {
        let mut config: PipelineConfig =
            crate::pipeline_config::workflow_template().unwrap().config;
        config.steps = vec![StepConfig {
            id: "review".into(),
            phase: Phase::Parallel,
            ..Default::default()
        }];
        config.merge.agents = vec!["claude".into()];
        config
    }

    fn run_folder() -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let run = directory.path().join("run");
        std::fs::create_dir_all(run.join("context")).unwrap();
        (directory, run)
    }

    #[test]
    fn a_run_without_a_saved_choice_keeps_its_settings_agents() {
        let (_directory, run) = run_folder();
        assert!(read_agent_overrides(&run).unwrap().is_none());
    }

    #[test]
    fn a_saved_choice_decides_the_resumed_agents_over_settings() {
        let (_directory, run) = run_folder();
        std::fs::write(
            run.join(AGENT_OVERRIDES_PATH),
            serde_json::to_string(&overrides()).unwrap(),
        )
        .unwrap();
        let saved = read_agent_overrides(&run).unwrap().unwrap();
        let mut settings = settings();
        let mut config = retained_workflow();
        reapply(&saved, &mut settings, &mut config).unwrap();
        apply_agent_defaults(&mut config, &settings);
        assert_eq!(config.steps[0].agents, ["codex"]);
        assert_eq!(settings.default_merge_agent, "codex");
        assert!(config.merge.agents.iter().all(|agent| agent == "codex"));
    }

    #[test]
    fn a_damaged_choice_stops_the_rerun_instead_of_using_other_agents() {
        let (_directory, run) = run_folder();
        std::fs::write(run.join(AGENT_OVERRIDES_PATH), "{ not json").unwrap();
        assert!(read_agent_overrides(&run)
            .unwrap_err()
            .contains("could not be read"));
    }

    #[test]
    fn a_choice_settings_no_longer_allow_stops_the_rerun() {
        let stale = RunModelOverrides {
            agents: vec!["gemini".into()],
            ..Default::default()
        };
        let error = reapply(&stale, &mut settings(), &mut retained_workflow()).unwrap_err();
        assert!(error.contains("can no longer be used"), "{error}");
    }

    #[test]
    fn the_notice_names_the_review_model() {
        let text = notice(&RunModelOverrides {
            review: Some(crate::commands::run_context::ReviewModelChoice {
                provider: "claude".into(),
                model: "claude-opus-5-5".into(),
                effort: "max".into(),
            }),
            ..Default::default()
        });
        assert!(
            text.ends_with("Review model: Claude Code, claude-opus-5-5, max thinking."),
            "{text}"
        );
    }

    #[test]
    fn the_notice_names_the_agents_in_researcher_terms() {
        let text = notice(&RunModelOverrides {
            agents: vec!["codex".into(), "claude".into()],
            merge_agent: Some("claude".into()),
            ..Default::default()
        });
        assert!(
            text.ends_with("Parallel: Codex, Claude Code. Merge: Claude Code."),
            "{text}"
        );
    }
}
