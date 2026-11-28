use super::{HarnessModule, HarnessPreset};

/// The researcher's style notes and past writing from About you. The built-in
/// Writing profile uses it whenever there is some, without listing it, so
/// conversations on that profile keep their fingerprint until there is.
pub(super) const OWN_WRITING: &str = "own_writing";

pub fn harness_modules() -> Vec<HarnessModule> {
    vec![
        HarnessModule { id: "paper_context", kind: "context_provider", version: 1, name: "Paper context", description: "Sends the project’s manuscript and notes with every message. Without it, a paper is sent only when you attach one.", requires_workspace: true, capability: "read", automatic: false },
        HarnessModule { id: OWN_WRITING, kind: "context_provider", version: 1, name: "Your writing", description: "Sends your style notes from About you and short passages from the papers you marked as examples, and lets the assistant read those papers. The Writing profile uses it whenever you have added some.", requires_workspace: false, capability: "read", automatic: false },
        HarnessModule { id: "task_tools", kind: "tool", version: 1, name: "Task chains", description: "Suggests tasks and follow-ups for you to review and start.", requires_workspace: true, capability: "propose", automatic: false },
        HarnessModule { id: "paper_tools", kind: "tool", version: 1, name: "Paper tools", description: "Reads and searches the project’s papers and sources. On in every project conversation.", requires_workspace: true, capability: "read", automatic: true },
        HarnessModule { id: "research_ledger", kind: "tool", version: 1, name: "Research ledger", description: "Suggests notes, claims, and evidence for you to review.", requires_workspace: true, capability: "propose", automatic: false },
        HarnessModule { id: "research_execution", kind: "tool", version: 1, name: "Research execution", description: "Runs only locally configured and tested execution profiles.", requires_workspace: true, capability: "execute", automatic: false },
        HarnessModule { id: "results_inspector", kind: "inspector", version: 1, name: "Results inspector", description: "Shows run history, outputs, errors, and result comparisons.", requires_workspace: true, capability: "inspect", automatic: false },
        HarnessModule { id: "evidence_inspector", kind: "inspector", version: 1, name: "Evidence inspector", description: "Shows supporting evidence, who checked it, and whether it is up to date.", requires_workspace: true, capability: "inspect", automatic: false },
    ]
}

/// Built-in profiles that were folded into the current lineup, and the profile
/// each one's conversations and copies now use. The audits they described are
/// recipes, and field conventions belong to the project.
const RETIRED_PRESETS: &[(&str, &str)] = &[
    ("research_assistant", "research"),
    ("econ_research", "research"),
    ("empirical_audit", "research"),
    ("theory_audit", "research"),
    ("literature_review", "research"),
    ("paper_revision", "writing"),
    ("code_review", "data_code"),
];

/// Instruction-pack modules that never added text or tools. Saved profiles may
/// still list them.
const RETIRED_MODULES: &[&str] = &["research_structure", "empirical_audit", "theory_audit"];

/// The profile a saved ID now means. Custom profile IDs are generated, so they
/// never collide with a retired built-in ID.
pub fn current_preset_id(id: String) -> String {
    RETIRED_PRESETS
        .iter()
        .find(|(retired, _)| *retired == id)
        .map_or(id, |(_, current)| (*current).to_string())
}

/// A saved module list without modules that no longer exist.
pub(super) fn current_modules(modules: Vec<String>) -> Vec<String> {
    modules
        .into_iter()
        .filter(|module| !RETIRED_MODULES.contains(&module.as_str()))
        .collect()
}

pub(super) fn builtin_presets() -> Vec<HarnessPreset> {
    let preset = |id: &str, name: &str, description: &str, instructions: &str, modules: &[&str]| {
        HarnessPreset {
            id: id.to_string(),
            workspace_id: None,
            name: name.to_string(),
            description: description.to_string(),
            base_instructions: None,
            instructions: instructions.trim().to_string(),
            modules: modules.iter().map(|value| (*value).to_string()).collect(),
            built_in: true,
            source_preset_id: None,
            revision: 1,
        }
    };
    vec![
        preset("plain", "Default", "The connection’s own system prompt with Pipeline’s project instructions. No added profile prompt or research tools.", "", &[]),
        preset("research", "Research", "Sharpen questions, design analyses, interpret results, and plan next steps.", include_str!("../../../prompts/agent_profiles/research.md"), &["paper_context", "research_ledger", "evidence_inspector"]),
        preset("writing", "Writing", "Draft, edit, and revise the manuscript and referee responses.", include_str!("../../../prompts/agent_profiles/writing.md"), &["paper_context"]),
        preset("data_code", "Data & code", "Write, check, and rerun analysis and replication code.", include_str!("../../../prompts/agent_profiles/data_code.md"), &["paper_context", "research_execution", "results_inspector"]),
    ]
}
