// Pure helpers for the Workspace harness editor: module grouping and
// capability bundles, preset summaries, access-inheritance rows, and
// instruction lint. No Tauri access; everything here is unit-testable.

import type {
  EffectiveHarness,
  HarnessModule,
  HarnessPreset,
  InstructionSection,
  ModuleAvailability,
} from "../../lib/workbenchTypes";

export type ModuleKind = HarnessModule["kind"];

export const MODULE_KIND_ORDER: ModuleKind[] = [
  "instruction_pack",
  "context_provider",
  "tool",
  "inspector",
  "recipe",
];

export const MODULE_KIND_LABELS: Record<ModuleKind, string> = {
  instruction_pack: "Instruction packs",
  context_provider: "Context",
  tool: "Tools",
  inspector: "Inspectors",
  recipe: "Recipes",
};

export const MODULE_KIND_HINTS: Record<ModuleKind, string> = {
  instruction_pack: "Instructions sent with each message.",
  context_provider: "Source material included with each message.",
  tool: "Tools the assistant can use. Each use is recorded.",
  inspector:
    "Research panels for you to view; these do not change the assistant’s instructions.",
  recipe: "Saved instructions for specific research tasks.",
};

export function groupModules(modules: HarnessModule[]): Array<{
  kind: ModuleKind;
  label: string;
  hint: string;
  modules: HarnessModule[];
}> {
  return MODULE_KIND_ORDER.map((kind) => ({
    kind,
    label: MODULE_KIND_LABELS[kind],
    hint: MODULE_KIND_HINTS[kind],
    modules: modules.filter((module) => module.kind === kind),
  })).filter((group) => group.modules.length > 0);
}

/** Capability bundles cover every module that is not an instruction pack. */
export interface ModuleBundle {
  id: string;
  label: string;
  description: string;
  modules: string[];
}

const BUNDLE_DEFINITIONS: Array<
  Omit<ModuleBundle, "modules"> & { modules: string[] }
> = [
  {
    id: "minimal",
    label: "Instructions only",
    description: "No context, tools, or inspectors.",
    modules: [],
  },
  {
    id: "read",
    label: "Read the paper",
    description: "Lets the assistant read and search the paper.",
    modules: ["paper_context", "paper_tools"],
  },
  {
    id: "ledger",
    label: "Paper + ledger",
    description:
      "Lets the assistant suggest notes, claims, and evidence for review.",
    modules: [
      "paper_context",
      "paper_tools",
      "research_ledger",
      "evidence_inspector",
    ],
  },
  {
    id: "execution",
    label: "Paper + ledger + execution",
    description: "Adds tested local commands and a panel to view results.",
    modules: [
      "paper_context",
      "paper_tools",
      "research_ledger",
      "evidence_inspector",
      "research_execution",
      "results_inspector",
    ],
  },
];

export function moduleBundles(catalog: HarnessModule[]): ModuleBundle[] {
  const known = new Set(catalog.map((module) => module.id));
  return BUNDLE_DEFINITIONS.map((bundle) => ({
    ...bundle,
    modules: bundle.modules.filter((id) => known.has(id)),
  }));
}

function isPack(id: string, catalog: HarnessModule[]) {
  return (
    catalog.find((module) => module.id === id)?.kind === "instruction_pack"
  );
}

/** Return `selected` in catalog order. Unknown ids are dropped. */
export function orderModules(
  selected: string[],
  catalog: HarnessModule[],
): string[] {
  const wanted = new Set(selected);
  return catalog
    .filter((module) => wanted.has(module.id))
    .map((module) => module.id);
}

/** Replace the non-pack modules with the bundle while keeping instruction packs. */
export function applyBundle(
  selected: string[],
  bundle: ModuleBundle,
  catalog: HarnessModule[],
): string[] {
  const packs = selected.filter((id) => isPack(id, catalog));
  return orderModules([...packs, ...bundle.modules], catalog);
}

export function activeBundle(
  selected: string[],
  bundles: ModuleBundle[],
  catalog: HarnessModule[],
): string | null {
  const capabilities = selected.filter((id) => !isPack(id, catalog)).sort();
  const match = bundles.find((bundle) => {
    const wanted = [...bundle.modules].sort();
    return (
      wanted.length === capabilities.length &&
      wanted.every((id, index) => id === capabilities[index])
    );
  });
  return match?.id ?? null;
}

export function toggleModule(
  selected: string[],
  id: string,
  catalog: HarnessModule[],
): string[] {
  return orderModules(
    selected.includes(id)
      ? selected.filter((item) => item !== id)
      : [...selected, id],
    catalog,
  );
}

function plural(count: number, singular: string, pluralForm = `${singular}s`) {
  return `${count} ${count === 1 ? singular : pluralForm}`;
}

function joinList(parts: string[]) {
  if (parts.length <= 1) return parts.join("");
  if (parts.length === 2) return `${parts[0]} and ${parts[1]}`;
  return `${parts.slice(0, -1).join(", ")}, and ${parts[parts.length - 1]}`;
}

/** One plain sentence describing what a module set does, in the style of the workflow editor's step summaries. */
export function describeModules(
  selected: string[],
  catalog: HarnessModule[],
  instructions: string,
): string {
  const modules = orderModules(selected, catalog).map((id) =>
    catalog.find((module) => module.id === id)!,
  );
  if (modules.length === 0) {
    return instructions.trim()
      ? "Preset instructions only; no automatic context, tools, or inspectors."
      : "No research instructions or automatic context; an ordinary conversation.";
  }
  const packs = modules.filter((module) => module.kind === "instruction_pack");
  const context = modules.filter(
    (module) => module.kind === "context_provider",
  );
  const tools = modules.filter((module) => module.kind === "tool");
  const inspectors = modules.filter((module) => module.kind === "inspector");
  const parts: string[] = [];
  if (packs.length) parts.push(plural(packs.length, "instruction pack"));
  if (context.length)
    parts.push(
      context.map((module) => module.name.toLowerCase()).join(" and "),
    );
  if (tools.length)
    parts.push(
      tools.length <= 2
        ? tools.map((module) => module.name.toLowerCase()).join(" and ")
        : plural(tools.length, "tool"),
    );
  if (inspectors.length) parts.push(plural(inspectors.length, "inspector"));
  const sentence = `Adds ${joinList(parts)}.`;
  return instructions.trim() ? sentence : `${sentence} No preset instructions.`;
}

export function describePreset(
  preset: HarnessPreset,
  catalog: HarnessModule[],
): string {
  return describeModules(
    preset.modules,
    catalog,
    preset.baseInstructions ?? preset.instructions,
  );
}

export function availabilityFor(
  id: string,
  availability: ModuleAvailability[] | undefined,
): ModuleAvailability {
  return (
    availability?.find((item) => item.id === id) ?? {
      id,
      available: true,
      reasons: [],
    }
  );
}

// --- Access inheritance -----------------------------------------------------

export type AccessScope = "builtIn" | "global" | "workspace" | "conversation";
export type AccessKey =
  "mode" | "commandNetwork" | "contextBudgetBytes" | "webSearch";

export const ACCESS_SCOPES: Array<{ id: AccessScope; label: string }> = [
  { id: "builtIn", label: "Built-in" },
  { id: "global", label: "Global" },
  { id: "workspace", label: "Workspace" },
  { id: "conversation", label: "This conversation" },
];

export interface AccessRow {
  key: AccessKey;
  label: string;
  description: string;
  builtIn: string | number | boolean;
  kind: "select" | "boolean";
  options?: Array<{ value: string | number; label: string }>;
  /** When set, no scope may override the value; the text explains why. */
  locked?: string;
}

export const ACCESS_ROWS: AccessRow[] = [
  {
    key: "mode",
    label: "Access mode",
    description:
      "Inspect lets commands read the registered folder. Edit also lets them write to it and enables execution modules.",
    builtIn: "inspect",
    kind: "select",
    options: [
      { value: "inspect", label: "Inspect" },
      { value: "edit", label: "Edit" },
    ],
  },
  {
    key: "commandNetwork",
    label: "Command network",
    description: "Whether commands the model runs may reach the network.",
    builtIn: false,
    kind: "boolean",
  },
  {
    key: "contextBudgetBytes",
    label: "Context budget",
    description:
      "Maximum amount of paper text and notes included with each message.",
    builtIn: 65536,
    kind: "select",
    options: [
      { value: 16384, label: "16 KiB" },
      { value: 65536, label: "64 KiB" },
      { value: 262144, label: "256 KiB" },
      { value: 524288, label: "512 KiB" },
    ],
  },
  {
    key: "webSearch",
    label: "Native web search",
    description: "Provider-side search inside the conversation.",
    builtIn: false,
    kind: "boolean",
    locked: "Web search is not available in Workspace yet.",
  },
];

export function formatAccessValue(row: AccessRow, value: unknown): string {
  if (value === undefined || value === null) return "—";
  if (row.kind === "boolean") return value === true ? "Allowed" : "Blocked";
  const option = row.options?.find((candidate) => candidate.value === value);
  return option?.label ?? String(value);
}

export function scopeValue(
  key: AccessKey,
  scope: AccessScope,
  bodies: {
    global: Record<string, unknown>;
    workspace: Record<string, unknown> | null;
    conversation: Record<string, unknown>;
  },
): unknown {
  if (scope === "builtIn")
    return ACCESS_ROWS.find((row) => row.key === key)?.builtIn;
  const body =
    scope === "global"
      ? bodies.global
      : scope === "workspace"
        ? bodies.workspace
        : bodies.conversation;
  const value = body?.[key];
  return value === null ? undefined : value;
}

export function winningScope(
  effective: EffectiveHarness,
  key: AccessKey,
): AccessScope {
  const source = effective.valueSources[key];
  return source === "global" ||
    source === "workspace" ||
    source === "conversation"
    ? source
    : "builtIn";
}

export function effectiveAccessValue(
  effective: EffectiveHarness,
  key: AccessKey,
): string | number | boolean {
  switch (key) {
    case "mode":
      return effective.mode;
    case "commandNetwork":
      return effective.commandNetwork;
    case "contextBudgetBytes":
      return effective.contextBudgetBytes;
    case "webSearch":
      return effective.webSearch;
  }
}

export function compactAccessSummary(effective: EffectiveHarness): string {
  const budget = ACCESS_ROWS.find((row) => row.key === "contextBudgetBytes")!;
  return [
    effective.mode === "edit" ? "Edit" : "Inspect",
    effective.commandNetwork ? "command network" : "no command network",
    formatAccessValue(budget, effective.contextBudgetBytes),
  ].join(" · ");
}

export function compactModuleSummary(effective: EffectiveHarness): string {
  const total = effective.preset.modules.length;
  const active = effective.enabledModules.length;
  const modules =
    total === 0
      ? "no modules"
      : active === total
        ? plural(total, "module") + " active"
        : `${active} of ${total} modules active`;
  return `${modules} · ${effective.permissionProfile}`;
}

// --- Instructions -----------------------------------------------------------

export const MAX_PRESET_INSTRUCTION_BYTES = 256 * 1024;
const LONG_INSTRUCTION_BYTES = 32 * 1024;

export interface InstructionLint {
  level: "warning" | "error";
  message: string;
}

export function byteLength(text: string): number {
  return new TextEncoder().encode(text).length;
}

export function lintHarnessInstructions(text: string): InstructionLint[] {
  const hits: InstructionLint[] = [];
  const bytes = byteLength(text);
  if (bytes > MAX_PRESET_INSTRUCTION_BYTES) {
    hits.push({
      level: "error",
      message: "Instructions exceed the 256 KiB limit and cannot be saved.",
    });
  } else if (bytes > LONG_INSTRUCTION_BYTES) {
    hits.push({
      level: "warning",
      message: `Long instructions (${Math.round(bytes / 1024)} KiB) are sent on every turn and reduce room for the paper.`,
    });
  }
  if (/<\/?workspace_context/i.test(text)) {
    hits.push({
      level: "warning",
      message:
        "<workspace_context> is reserved for the host; Pipeline wraps source material itself and escapes it.",
    });
  }
  if (
    /\{(?:step:|orientation\}|prior_outputs\}|last_output\}|input_path\}|var:|input:)/.test(
      text,
    )
  ) {
    hits.push({
      level: "warning",
      message:
        "Workflow placeholders such as {step:id} are not expanded in Workspace instructions; they reach the model verbatim.",
    });
  }
  return hits;
}

export function instructionSections(
  effective: EffectiveHarness,
): InstructionSection[] {
  if (effective.instructionSections?.length)
    return effective.instructionSections;
  return [
    {
      id: "all",
      label: "Developer instructions",
      text: effective.developerInstructions,
    },
  ];
}

/** Built-in preset instruction blocks offered as insertable starting points. */
export function instructionBlocks(
  presets: HarnessPreset[],
): Array<{ id: string; label: string; text: string }> {
  return presets
    .filter((preset) => preset.builtIn && preset.instructions.trim())
    .map((preset) => ({
      id: preset.id,
      label: preset.name,
      text: preset.instructions.trim(),
    }));
}

export function compactHash(value?: string | null): string {
  return value ? `${value.slice(0, 10)}…${value.slice(-6)}` : "—";
}
