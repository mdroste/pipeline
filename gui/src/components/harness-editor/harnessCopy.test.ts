import { expect, it } from "vitest";
import type {
  HarnessModule,
  HarnessPreset,
} from "../../lib/workbench/harnessTypes";
import {
  basePromptOwner,
  harnessDiagnostic,
  moduleDescription,
  moduleName,
  presetDescription,
  systemPromptNote,
  systemPromptSource,
} from "./harnessCopy";

it("changes catalog display text without changing saved configuration metadata", () => {
  const module: HarnessModule = Object.freeze({
    id: "research_execution",
    kind: "tool",
    version: 1,
    name: "Research execution",
    description: "Runs only locally configured and tested execution profiles.",
    capability: "execute",
    requiresWorkspace: true,
  });
  const preset: HarnessPreset = Object.freeze({
    id: "research",
    workspaceId: null,
    name: "Research",
    description:
      "Sharpen questions, design analyses, interpret results, and plan next steps.",
    instructions: "Work as a research collaborator on this project.",
    modules: [],
    builtIn: true,
    sourcePresetId: null,
    revision: 1,
  });
  const stored = JSON.stringify({ module, preset });

  expect(moduleName(module)).toBe("Local analysis");
  expect(moduleDescription(module)).toBe(
    "Run commands you have configured and tested on this computer.",
  );
  // Built-in descriptions without an alias pass through unchanged.
  expect(presetDescription(preset)).toBe(preset.description);
  expect(JSON.stringify({ module, preset })).toBe(stored);
  expect(presetDescription({ ...preset, builtIn: false })).toBe(
    preset.description,
  );
});

it("says whose base prompt a profile keeps or replaces on each connection", () => {
  const plain = { instructions: "", baseInstructions: null };
  const adds = { instructions: "Check units.", baseInstructions: null };
  const replaces = { instructions: "", baseInstructions: "You are an editor." };

  expect(systemPromptNote(plain, "codex")).toBe(
    "Keeps Codex’s built-in base prompt and adds Pipeline’s instructions.",
  );
  expect(systemPromptNote(adds, "claude-code-api")).toBe(
    "Keeps Claude Code’s built-in base prompt and adds Pipeline’s instructions and this profile’s instructions.",
  );
  expect(systemPromptNote(replaces, "claude")).toBe(
    "Replaces Claude Code’s built-in base prompt with this profile’s prompt. Pipeline’s instructions are still added.",
  );
  expect(systemPromptNote(plain, "google-api")).toBe(
    "Starts from Pipeline’s one-line base prompt and adds Pipeline’s instructions.",
  );
  expect(systemPromptNote(replaces, "local")).toBe(
    "Uses this profile’s prompt as the base prompt, followed by Pipeline’s instructions.",
  );
  // An empty replacement still replaces; only null inherits.
  expect(
    systemPromptNote({ instructions: "", baseInstructions: "" }, "codex"),
  ).toMatch(/^Replaces Codex’s/);

  expect(systemPromptSource(plain, "codex")).toBe("Codex built-in");
  expect(systemPromptSource(adds, "anthropic-api")).toBe(
    "Pipeline one-line default",
  );
  expect(systemPromptSource(replaces, "codex")).toBe("This profile’s prompt");
  expect(basePromptOwner("openai-api")).toBeNull();
});

it("keeps unfamiliar diagnostics intact", () => {
  expect(harnessDiagnostic("A command failed: exit 2")).toBe(
    "A command failed: exit 2",
  );
  expect(harnessDiagnostic("constructor")).toBe("constructor");
});
