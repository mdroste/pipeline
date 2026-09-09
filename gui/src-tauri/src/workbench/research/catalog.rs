use super::{HarnessModule, HarnessPreset};

pub fn harness_modules() -> Vec<HarnessModule> {
    vec![
        HarnessModule { id: "research_structure", kind: "instruction_pack", version: 1, name: "Research structure", description: "Separates questions, assumptions, mechanisms, evidence, and uncertainty.", requires_workspace: false, capability: "instructions" },
        HarnessModule { id: "empirical_audit", kind: "instruction_pack", version: 1, name: "Empirical audit", description: "Tracks estimands, identifying variation, samples, inference, and result consistency.", requires_workspace: false, capability: "instructions" },
        HarnessModule { id: "theory_audit", kind: "instruction_pack", version: 1, name: "Theory audit", description: "Tracks primitives, timing, equilibrium, units, limiting cases, and comparative statics.", requires_workspace: false, capability: "instructions" },
        HarnessModule { id: "paper_context", kind: "context_provider", version: 1, name: "Paper context", description: "Includes the selected paper version and project notes.", requires_workspace: true, capability: "read" },
        HarnessModule { id: "task_tools", kind: "tool", version: 1, name: "Task chains", description: "Suggests tasks and follow-ups for you to review and start.", requires_workspace: true, capability: "propose" },
        HarnessModule { id: "paper_tools", kind: "tool", version: 1, name: "Paper tools", description: "Reads and searches the selected paper.", requires_workspace: true, capability: "read" },
        HarnessModule { id: "research_ledger", kind: "tool", version: 1, name: "Research ledger", description: "Suggests notes, claims, and evidence for you to review.", requires_workspace: true, capability: "propose" },
        HarnessModule { id: "research_execution", kind: "tool", version: 1, name: "Research execution", description: "Runs only locally configured and tested execution profiles.", requires_workspace: true, capability: "execute" },
        HarnessModule { id: "results_inspector", kind: "inspector", version: 1, name: "Results inspector", description: "Shows run history, outputs, errors, and result comparisons.", requires_workspace: true, capability: "inspect" },
        HarnessModule { id: "evidence_inspector", kind: "inspector", version: 1, name: "Evidence inspector", description: "Shows supporting evidence, who checked it, and whether it is up to date.", requires_workspace: true, capability: "inspect" },
    ]
}

pub(super) fn builtin_presets() -> Vec<HarnessPreset> {
    let preset = |id: &str, name: &str, description: &str, instructions: &str, modules: &[&str]| {
        HarnessPreset {
            id: id.to_string(),
            workspace_id: None,
            name: name.to_string(),
            description: description.to_string(),
            base_instructions: matches!(id, "writing" | "code_review" | "econ_research")
                .then(|| instructions.to_string()),
            instructions: if matches!(id, "writing" | "code_review" | "econ_research") {
                String::new()
            } else {
                instructions.to_string()
            },
            modules: modules.iter().map(|value| (*value).to_string()).collect(),
            built_in: true,
            source_preset_id: None,
            revision: 1,
        }
    };
    vec![
        preset("plain", "Codex default", "Codex with Pipeline Workspace instructions and no added profile prompt or research tools.", "", &[]),
        preset("writing", "Writing", "Draft and revise clear prose while preserving the author’s meaning and voice.", include_str!("../../../../../prompts/agent_profiles/writing.md"), &["paper_context", "paper_tools"]),
        preset("code_review", "Code review", "Review code for actionable defects, regressions, and missing coverage.", include_str!("../../../../../prompts/agent_profiles/code_review.md"), &[]),
        preset("econ_research", "Economics research", "Develop economic arguments and evaluate identification, mechanisms, and evidence.", include_str!("../../../../../prompts/agent_profiles/econ_research.md"), &["research_structure", "paper_context", "paper_tools", "research_ledger", "evidence_inspector"]),
        preset("research_assistant", "Research assistant", "General-purpose academic research structure.", "State the research question precisely. Separate assumptions, mechanisms, evidence, and remaining uncertainty. Be concise and preserve economically meaningful objects.", &["research_structure", "paper_context", "paper_tools", "research_ledger", "evidence_inspector"]),
        preset("empirical_audit", "Empirical audit", "Audit design, samples, inference, and reported results.", "Identify the estimand and identifying variation. Track assignment and inference levels, samples, weights, specifications, and consistency between results and prose. Do not infer identification from statistical significance.", &["research_structure", "empirical_audit", "paper_context", "paper_tools", "research_ledger", "research_execution", "results_inspector", "evidence_inspector"]),
        preset("theory_audit", "Theory audit", "Audit model logic and comparative statics.", "State primitives, timing, optimization, equilibrium and accounting conditions. Check units, limiting cases, and comparative statics. Distinguish assumptions from results.", &["research_structure", "theory_audit", "paper_context", "paper_tools", "research_ledger", "research_execution", "results_inspector", "evidence_inspector"]),
        preset("literature_review", "Literature review", "Inspect primary sources and record search coverage.", "Separate source identity and access from evidence. Prefer primary sources, record search coverage, and label novelty statements as conjectures unless established.", &["research_structure", "paper_context", "paper_tools", "research_ledger", "evidence_inspector"]),
        preset("paper_revision", "Paper revision", "Revise against an immutable manuscript revision.", "Preserve substantive claims, equations, economic logic, and empirical claims unless asked to change them. Tie edits to manuscript revisions and recorded checks.", &["research_structure", "paper_context", "paper_tools", "research_ledger", "research_execution", "results_inspector", "evidence_inspector"]),
    ]
}
