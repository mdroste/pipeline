import type {
  HarnessModule,
  HarnessPreset,
} from "../../lib/workbench/harnessTypes";
import type { ConversationProvider } from "../../lib/workspaceProviders";

// Display aliases only: the backend metadata is part of saved harness
// fingerprints and must remain unchanged when interface wording changes.
const displayCopy: Readonly<Record<string, string>> = {
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

/**
 * Who writes the connection's built-in base prompt. The ChatGPT connection
 * runs Codex and both Claude Code connections run Claude Code. API and local
 * connections have none, so Pipeline starts them from one line (workbench/api).
 */
export function basePromptOwner(
  runtime: ConversationProvider,
): "Codex" | "Claude Code" | null {
  if (runtime === "codex") return "Codex";
  if (runtime === "claude" || runtime === "claude-code-api")
    return "Claude Code";
  return null;
}

type PromptFields = Pick<HarnessPreset, "instructions" | "baseInstructions">;

/** What a profile does to the connection's base prompt, in one sentence. */
export function systemPromptNote(
  preset: PromptFields,
  runtime: ConversationProvider,
): string {
  const owner = basePromptOwner(runtime);
  if (preset.baseInstructions != null) {
    return owner
      ? `Replaces ${owner}’s built-in base prompt with this profile’s prompt. Pipeline’s instructions are still added.`
      : "Uses this profile’s prompt as the base prompt, followed by Pipeline’s instructions.";
  }
  const added = preset.instructions.trim()
    ? " and this profile’s instructions"
    : "";
  return owner
    ? `Keeps ${owner}’s built-in base prompt and adds Pipeline’s instructions${added}.`
    : `Starts from Pipeline’s one-line base prompt and adds Pipeline’s instructions${added}.`;
}

/** Short form of {@link systemPromptNote} for compact summaries. */
export function systemPromptSource(
  preset: PromptFields,
  runtime: ConversationProvider,
): string {
  if (preset.baseInstructions != null) return "This profile’s prompt";
  const owner = basePromptOwner(runtime);
  return owner ? `${owner} built-in` : "Pipeline one-line default";
}
