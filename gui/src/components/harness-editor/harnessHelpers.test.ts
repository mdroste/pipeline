import { describe, expect, it } from "vitest";
import type { EffectiveHarness, HarnessModule, HarnessPreset } from "../../lib/workbenchTypes";
import {
  activeBundle,
  applyBundle,
  compactAccessSummary,
  compactModuleSummary,
  describeModules,
  describePreset,
  groupModules,
  instructionSections,
  lintHarnessInstructions,
  moduleBundles,
  scopeValue,
  toggleModule,
  winningScope,
} from "./harnessHelpers";

const module = (id: string, kind: HarnessModule["kind"], name: string, capability = "read", requiresWorkspace = true): HarnessModule =>
  ({ id, kind, version: 1, name, description: "", requiresWorkspace, capability });

export const CATALOG: HarnessModule[] = [
  module("research_structure", "instruction_pack", "Research structure", "instructions", false),
  module("empirical_audit", "instruction_pack", "Empirical audit", "instructions", false),
  module("paper_context", "context_provider", "Paper context"),
  module("paper_tools", "tool", "Paper tools"),
  module("research_ledger", "tool", "Research ledger", "propose"),
  module("research_execution", "tool", "Research execution", "execute"),
  module("results_inspector", "inspector", "Results inspector", "inspect"),
  module("evidence_inspector", "inspector", "Evidence inspector", "inspect"),
];

const preset = (overrides: Partial<HarnessPreset>): HarnessPreset => ({
  id: "p", workspaceId: null, name: "P", description: "", instructions: "", modules: [], builtIn: true, sourcePresetId: null, revision: 1, ...overrides,
});

const effective = (overrides: Partial<EffectiveHarness>): EffectiveHarness => ({
  schemaVersion: 1, sessionId: "s", workspaceId: "w", preset: preset({ modules: ["paper_context", "paper_tools"] }), mode: "inspect", webSearch: false,
  commandNetwork: false, permissionProfile: "workbench-inspect", contextBudgetBytes: 65536, enabledModules: ["paper_context"], unavailableModules: ["paper_tools"],
  diagnostics: [], developerInstructions: "all", contextPreview: "", contextTruncated: false, dynamicTools: [], fingerprint: "f", valueSources: {}, ...overrides,
});

describe("module grouping and bundles", () => {
  it("groups by kind in a fixed order and drops empty kinds", () => {
    expect(groupModules(CATALOG).map((group) => group.kind)).toEqual(["instruction_pack", "context_provider", "tool", "inspector"]);
  });

  it("applies a bundle while keeping instruction packs, in catalog order", () => {
    const bundles = moduleBundles(CATALOG);
    const ledger = bundles.find((bundle) => bundle.id === "ledger")!;
    const next = applyBundle(["research_execution", "empirical_audit", "research_structure"], ledger, CATALOG);
    expect(next).toEqual(["research_structure", "empirical_audit", "paper_context", "paper_tools", "research_ledger", "evidence_inspector"]);
    expect(activeBundle(next, bundles, CATALOG)).toBe("ledger");
    expect(activeBundle([...next, "results_inspector"], bundles, CATALOG)).toBeNull();
  });

  it("toggles a module and keeps catalog order", () => {
    expect(toggleModule(["paper_tools"], "paper_context", CATALOG)).toEqual(["paper_context", "paper_tools"]);
    expect(toggleModule(["paper_context", "paper_tools"], "paper_context", CATALOG)).toEqual(["paper_tools"]);
  });
});

describe("summaries", () => {
  it("describes module sets in one sentence", () => {
    expect(describeModules([], CATALOG, "")).toBe("No research instructions or automatic context; an ordinary conversation.");
    expect(describeModules([], CATALOG, "Be concise.")).toBe("Preset instructions only; no automatic context, tools, or inspectors.");
    expect(describePreset(preset({ modules: ["research_structure", "paper_context", "paper_tools", "research_ledger", "evidence_inspector"], instructions: "x" }), CATALOG))
      .toBe("Adds 1 instruction pack, paper context, paper tools and research ledger, and 1 inspector.");
    expect(describeModules(["research_structure", "empirical_audit", "paper_tools", "research_ledger", "research_execution"], CATALOG, ""))
      .toBe("Adds 2 instruction packs and 3 tools. No preset instructions.");
  });

  it("summarizes access and module counts compactly", () => {
    const value = effective({ mode: "edit", commandNetwork: true, contextBudgetBytes: 262144 });
    expect(compactAccessSummary(value)).toBe("Edit · command network · 256 KiB");
    expect(compactModuleSummary(value)).toBe("1 of 2 modules active · workbench-inspect");
    expect(compactModuleSummary(effective({ preset: preset({}), enabledModules: [], unavailableModules: [] }))).toBe("no modules · workbench-inspect");
  });
});

describe("access inheritance", () => {
  it("reads the winning scope from value sources and falls back to built-in", () => {
    const value = effective({ valueSources: { mode: "conversation", contextBudgetBytes: "global", commandNetwork: "unknown" } });
    expect(winningScope(value, "mode")).toBe("conversation");
    expect(winningScope(value, "contextBudgetBytes")).toBe("global");
    expect(winningScope(value, "commandNetwork")).toBe("builtIn");
    expect(winningScope(value, "webSearch")).toBe("builtIn");
  });

  it("treats null bodies as inherited and exposes built-in defaults", () => {
    const bodies = { global: { mode: "edit", commandNetwork: null }, workspace: null, conversation: {} };
    expect(scopeValue("mode", "global", bodies)).toBe("edit");
    expect(scopeValue("commandNetwork", "global", bodies)).toBeUndefined();
    expect(scopeValue("mode", "workspace", bodies)).toBeUndefined();
    expect(scopeValue("contextBudgetBytes", "builtIn", bodies)).toBe(65536);
  });
});

describe("instructions", () => {
  it("lints reserved tags, workflow placeholders, and size", () => {
    expect(lintHarnessInstructions("Be precise.")).toEqual([]);
    expect(lintHarnessInstructions("<workspace_context>x</workspace_context>").map((hit) => hit.level)).toEqual(["warning"]);
    expect(lintHarnessInstructions("Use {step:consolidate} here")[0].message).toMatch(/not expanded/);
    expect(lintHarnessInstructions("a".repeat(40 * 1024))[0].message).toMatch(/Long instructions/);
    expect(lintHarnessInstructions("a".repeat(257 * 1024))[0].level).toBe("error");
  });

  it("falls back to one band when the backend reports no sections", () => {
    expect(instructionSections(effective({}))).toEqual([{ id: "all", label: "Developer instructions", text: "all" }]);
    const sections = [{ id: "preamble", label: "Preamble", text: "p" }];
    expect(instructionSections(effective({ instructionSections: sections }))).toBe(sections);
  });
});
