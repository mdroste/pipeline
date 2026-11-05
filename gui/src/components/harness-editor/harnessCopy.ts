import type { HarnessModule, HarnessPreset } from "../../lib/workbenchTypes";

// Display aliases only: the backend metadata is part of saved harness
// fingerprints and must remain unchanged when interface wording changes.
const displayCopy: Readonly<Record<string, string>> = {
  "General-purpose academic research structure.":
    "Help develop research questions, assess evidence, and organize findings.",
  "Revise against an immutable manuscript revision.":
    "Revise a saved version of the manuscript.",
  "Codex with Pipeline Workspace instructions and no added profile prompt or research tools.":
    "Use Codex with Pipeline’s standard instructions and no additional research tools.",
  "Task chains": "Automations",
  "Research ledger": "Notes, claims & evidence",
  "Research execution": "Local analysis",
  "Runs only locally configured and tested execution profiles.":
    "Run commands you have configured and tested on this computer.",
  "Results inspector": "Results",
  "Evidence inspector": "Evidence",
  "Native web-search control is unavailable in the pinned App Server protocol; it remains disabled.":
    "Web search is unavailable with this version of the connection.",
  "Research execution is disabled until an execution profile passes its test action.":
    "Test a command successfully before using it for research.",
};

export function harnessDiagnostic(text: string): string {
  return Object.prototype.hasOwnProperty.call(displayCopy, text)
    ? displayCopy[text]
    : text;
}

export function presetDescription(preset: HarnessPreset): string {
  return preset.builtIn
    ? harnessDiagnostic(preset.description)
    : preset.description;
}

export function moduleName(module: HarnessModule): string {
  return harnessDiagnostic(module.name);
}

export function moduleDescription(module: HarnessModule): string {
  return harnessDiagnostic(module.description);
}
