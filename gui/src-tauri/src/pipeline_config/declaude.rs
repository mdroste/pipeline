//! Declaude's editorial workflow. Subject IDs and descriptions are snapshotted
//! from the curated catalog into each saved profile; they grant no tools or
//! scientific-review instructions. Three fixed slots read an ordered, unique
//! list from orientation, so ordinary survey guards own selection and resume.
//! Structured candidates remain inspectable while the final independent check
//! publishes Markdown. Explicit slot references expose missing reports, allowing
//! the editor to distinguish an unselected slot from incomplete selected coverage.

use super::*;

mod schemas;
#[cfg(test)]
mod tests;

pub(super) const DECLAUDE_ID: &str = "declaude";
const GENERAL_TOPIC: &str = "general_topic";
const REFEREES: [&str; 3] = [
    "declaude_referee_1",
    "declaude_referee_2",
    "declaude_referee_3",
];
const EDITOR: &str = "declaude_editor";
const REPORT: &str = "declaude_report";
const CONTRACT: &str = include_str!("../../prompts/declaude/contract.md");
const ORIENTATION: &str = include_str!("../../prompts/declaude/orientation.md");
const REVIEWER: &str = include_str!("../../prompts/declaude/reviewer.md");
const CONSOLIDATE: &str = include_str!("../../prompts/declaude/consolidate.md");
const VERIFY: &str = include_str!("../../prompts/declaude/verify.md");

fn orientation_prompt() -> String {
    let mut prompt = ORIENTATION.to_string();
    prompt.push_str("\n\nALLOWED TOPIC REFEREES\n");
    prompt.push_str(&format!(
        "- {GENERAL_TOPIC}: General topic editor. Use when no listed subject fits.\n"
    ));
    for subject in crate::auto_review::SUBJECTS {
        prompt.push_str(&format!(
            "- {}: {}. {} Exclude when: {}\n",
            subject.id, subject.label, subject.routing_description, subject.routing_exclusions
        ));
    }
    prompt
}

fn source_context() -> Vec<ArtifactSelector> {
    vec![
        ArtifactSelector::Primary {
            parts: vec![
                PrimaryArtifactPart::Text,
                PrimaryArtifactPart::Structure,
                PrimaryArtifactPart::Visuals,
                PrimaryArtifactPart::Source,
            ],
        },
        ArtifactSelector::Survey,
    ]
}

fn editorial_step(id: &str, label: &str, phase: Phase, prompt: String) -> StepConfig {
    StepConfig {
        id: id.into(),
        label: label.into(),
        prompt,
        system_prompt: CONTRACT.into(),
        phase,
        context: StepContext {
            include: source_context(),
        },
        ..Default::default()
    }
}

fn add_reports(step: &mut StepConfig, producers: &[&str]) {
    step.context
        .include
        .extend(producers.iter().map(|id| ArtifactSelector::Step {
            step: (*id).into(),
            parts: vec![StepArtifactPart::Report],
            glob: String::new(),
        }));
}

pub(super) fn declaude_profile() -> ProfileData {
    let mut steps = Vec::new();
    for (index, id) in REFEREES.iter().enumerate() {
        let mut step = editorial_step(
            id,
            &format!("Topic Referee {}", index + 1),
            Phase::Parallel,
            format!(
                "Your assigned topic is the subject ID at /referees/{index} in the orientation map. \
                 Use its selection_notes and section map as context. {}\n\n{REVIEWER}",
                if index == 0 {
                    "Review prose throughout the available document."
                } else {
                    "Concentrate on passages where your topic expertise matters, including nearby context."
                }
            ),
        );
        if index > 0 {
            step.run_if = Some(RunCondition::SurveyPath {
                pointer: format!("/referees/{index}"),
                exists: Some(true),
                equals: None,
                contains: None,
            });
        }
        step.output_schema = Some(schemas::edits_schema());
        steps.push(step);
    }
    let mut editor = editorial_step(
        EDITOR,
        "Consolidate Prose Edits",
        Phase::Sequential,
        CONSOLIDATE.into(),
    );
    add_reports(&mut editor, &REFEREES);
    editor.dependency_policy.required = vec![REFEREES[0].into()];
    editor.output_schema = Some(schemas::edits_schema());
    steps.push(editor);

    let mut report = editorial_step(
        REPORT,
        "Verify Replacements & Write Report",
        Phase::Sequential,
        VERIFY.into(),
    );
    add_reports(&mut report, &REFEREES);
    add_reports(&mut report, &[EDITOR]);
    report.dependency_policy.required = vec![REFEREES[0].into(), EDITOR.into()];
    steps.push(report);

    let mut profile = ProfileData::new(
        "Declaude",
        steps,
        MergeConfig {
            enabled: false,
            prompt: String::new(),
            agents: vec![],
        },
    );
    profile.context_cache.enabled = true;
    profile.orientation_prompt = orientation_prompt();
    profile.orientation_schema = Some(schemas::orientation_schema());
    profile.parallel_context_template = include_str!("../../prompts/declaude/context.md").into();
    profile.extraction.input_mode = "document".into();
    profile.outputs.primary_step = REPORT.into();
    profile
}
