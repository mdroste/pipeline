import { expect, it } from "vitest";
import type { HarnessModule, HarnessPreset } from "../../lib/workbenchTypes";
import {
  harnessDiagnostic,
  moduleDescription,
  moduleName,
  presetDescription,
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
    id: "plain",
    workspaceId: null,
    name: "Codex default",
    description:
      "Codex with Pipeline Workspace instructions and no added profile prompt or research tools.",
    instructions: "",
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
  expect(presetDescription(preset)).toBe(
    "Use Codex with Pipeline’s standard instructions and no additional research tools.",
  );
  expect(JSON.stringify({ module, preset })).toBe(stored);
  expect(presetDescription({ ...preset, builtIn: false })).toBe(
    preset.description,
  );
});

it("keeps unfamiliar diagnostics intact", () => {
  expect(harnessDiagnostic("A command failed: exit 2")).toBe(
    "A command failed: exit 2",
  );
  expect(harnessDiagnostic("constructor")).toBe("constructor");
});
