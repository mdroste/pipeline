import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceHarnessEditor from "./WorkspaceHarnessEditor";
import type { ConversationSnapshot, EffectiveHarness, HarnessCatalog, HarnessPreset } from "../lib/workbenchTypes";
import { CATALOG } from "./harness-editor/harnessHelpers.test";

const mocks = vi.hoisted(() => ({
  nativePromptCatalog: vi.fn(), harnessCatalog: vi.fn(), effectiveHarness: vi.fn(), getWorkspaceConfig: vi.fn(), saveWorkspaceConfig: vi.fn(),
  clonePreset: vi.fn(), updatePreset: vi.fn(), conversationSnapshot: vi.fn(), updateSession: vi.fn(),
  confirmDialog: vi.fn(),
}));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("./DialogService", () => ({ confirmDialog: mocks.confirmDialog, notify: vi.fn() }));

const builtIn = (id: string, name: string, instructions: string, modules: string[]): HarnessPreset =>
  ({ id, workspaceId: null, name, description: `${name} description`, instructions, modules, builtIn: true, sourcePresetId: null, revision: 1 });

const custom: HarnessPreset = {
  id: "preset-custom", workspaceId: "workspace", name: "My audit", description: "Mine", instructions: "Check the estimand.",
  modules: ["research_structure", "empirical_audit", "paper_context", "paper_tools", "research_execution"], builtIn: false, sourcePresetId: "empirical_audit", revision: 2,
};

function catalog(): HarnessCatalog {
  return {
    schemaVersion: 1, toolCatalogVersion: 4, modules: CATALOG,
    promptLayers: [{ id: "preamble", label: "Pipeline Workspace instructions", text: "Always preserve the native base prompt." }],
    presets: [
      builtIn("plain", "Codex default", "", []),
      builtIn("research_assistant", "Research assistant", "State the question precisely.", ["research_structure", "paper_context", "paper_tools", "research_ledger", "evidence_inspector"]),
      builtIn("empirical_audit", "Empirical audit", "Identify the estimand.", ["research_structure", "empirical_audit", "paper_context", "paper_tools", "research_ledger", "research_execution", "results_inspector", "evidence_inspector"]),
      custom,
    ],
  };
}

function effective(presetId = "empirical_audit"): EffectiveHarness {
  const preset = catalog().presets.find((item) => item.id === presetId)!;
  return {
    schemaVersion: 1, sessionId: "session", workspaceId: "workspace", preset, mode: "inspect", webSearch: false, commandNetwork: true,
    permissionProfile: "workbench-inspect-network", contextBudgetBytes: 262144,
    enabledModules: preset.modules.filter((id) => id !== "research_execution"), unavailableModules: preset.modules.includes("research_execution") ? ["research_execution"] : [],
    diagnostics: [], developerInstructions: "preamble\n\npreset", contextPreview: "", contextTruncated: false, dynamicTools: [{ name: "workbench_paper_read" }],
    fingerprint: "fingerprint-b",
    valueSources: { mode: "builtIn", commandNetwork: "conversation", contextBudgetBytes: "global", webSearch: "builtIn" },
    moduleAvailability: CATALOG.map((module) => module.id === "research_execution"
      ? { id: module.id, available: false, reasons: ["Needs Edit access mode.", "Needs an execution profile that has passed its test."] }
      : { id: module.id, available: true, reasons: [] }),
    instructionSections: [{ id: "preamble", label: "Pipeline instructions", text: "preamble" }, { id: "preset", label: "Preset instructions: Empirical audit", text: "preset" }],
  };
}

function snapshot(presetId = "empirical_audit"): ConversationSnapshot {
  return {
    workspace: { id: "workspace", name: "Local research", root: "/research", rootIdentity: null, settingsRevision: 1, revision: 1, archivedAt: null, missingRootAt: null, createdAt: "now", updatedAt: "now" },
    session: { id: "session", workspaceId: "workspace", paperId: null, title: "Question", presetId, overrides: { commandNetwork: false }, draft: "", revision: 3, archivedAt: null, createdAt: "now", updatedAt: "now" },
    sequence: 1,
    activeBinding: { id: "binding", sessionId: "session", runtimeNamespace: "ns", providerThreadId: "thread", incarnation: 1, createdAt: "now", retiredAt: null, retirementReason: null, harnessFingerprint: "fingerprint-a", instructionSources: ["AGENTS.md"] },
    turns: [], items: [],
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  mocks.nativePromptCatalog.mockResolvedValue({ installedVersion: "0.153.4", sources: [{ path: "/codex/models_cache.json", origin: "codex", clientVersion: "0.153.4", fetchedAt: "today", models: [{ model: "model-a", template: "Cached base template", templateField: "model_messages.instructions_template", sections: [] }] }], diagnostics: [] });
  mocks.harnessCatalog.mockResolvedValue(catalog());
  mocks.effectiveHarness.mockResolvedValue(effective());
  mocks.getWorkspaceConfig.mockImplementation(async (workspaceId: string | null) => workspaceId
    ? { scopeKey: "workspace", workspaceId, schemaVersion: 1, body: { commandNetwork: true }, revision: 4, updatedAt: "now" }
    : { scopeKey: "global", workspaceId: null, schemaVersion: 1, body: { contextBudgetBytes: 262144 }, revision: 2, updatedAt: "now" });
  mocks.saveWorkspaceConfig.mockResolvedValue({});
  mocks.conversationSnapshot.mockImplementation(async () => snapshot());
  mocks.updateSession.mockImplementation(async (request: { presetId?: string; overrides?: Record<string, unknown> }) => ({ record: { ...snapshot().session, presetId: request.presetId ?? "empirical_audit", overrides: request.overrides ?? {} }, sequence: 2 }));
  mocks.clonePreset.mockResolvedValue({ ...custom, id: "preset-new", name: "Empirical audit copy", revision: 1 });
  mocks.updatePreset.mockResolvedValue({ ...custom, revision: 3 });
  mocks.confirmDialog.mockResolvedValue(true);
});
afterEach(cleanup);

function renderEditor(props: Partial<{ snapshot: ConversationSnapshot; onSnapshot: (next: ConversationSnapshot) => void; onClose: () => void }> = {}) {
  return render(<WorkspaceHarnessEditor snapshot={props.snapshot ?? snapshot()} onSnapshot={props.onSnapshot ?? vi.fn()} onClose={props.onClose ?? vi.fn()} />);
}

it("lists presets, opens the preset in use as read-only, and flags the successor thread", async () => {
  renderEditor();
  const heading = await screen.findByRole("heading", { name: "Empirical audit" });
  expect(heading).toBeInTheDocument();
  expect(screen.getByText("In use by this conversation")).toBeInTheDocument();
  expect(screen.getByText(/Built-in profiles are read-only/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Use in this conversation" })).not.toBeInTheDocument();
  expect(screen.getByTestId("preset-summary")).toHaveTextContent("Adds 2 instruction packs, paper context, 3 tools, and 2 inspectors.");
  expect(screen.getAllByText("In use")).toHaveLength(1);
  expect(screen.getByText(/assistant will start fresh/)).toBeInTheDocument();
  expect(screen.getByText("Your profiles")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /My audit/ })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("tab", { name: "Tools & context" }));
  expect(screen.getByRole("switch", { name: "Research execution" })).toBeDisabled();
  expect(screen.getByText(/Needs Edit access mode/)).toBeInTheDocument();
});

it("clones a built-in through a named dialog and switches the conversation to the copy", async () => {
  const onSnapshot = vi.fn();
  renderEditor({ onSnapshot });
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: "Duplicate profile" }));
  const dialog = await screen.findByRole("dialog");
  const input = within(dialog).getByRole("textbox");
  fireEvent.change(input, { target: { value: "Field audit" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Create profile" }));
  await waitFor(() => expect(mocks.clonePreset).toHaveBeenCalledWith(expect.objectContaining({ workspaceId: "workspace", sourcePresetId: "empirical_audit", name: "Field audit" })));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ presetId: "preset-new", expectedRevision: 3 })));
  expect(onSnapshot).toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("edits a custom preset as a draft, saves with the expected revision, and guards navigation", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /My audit/ }));
  const name = await screen.findByLabelText("Profile name");
  expect(name).toHaveValue("My audit");
  expect(screen.getByText("based on Empirical audit")).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Supplemental instructions"), { target: { value: "Check the estimand. <workspace_context>" } });
  expect(screen.getByText(/reserved for the host/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("tab", { name: "Tools & context" }));
  fireEvent.click(screen.getByRole("switch", { name: "Evidence inspector" }));
  fireEvent.click(screen.getByRole("button", { name: "Paper + ledger" }));
  expect(screen.getByText("Unsaved")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Use in this conversation" })).toBeDisabled();

  mocks.confirmDialog.mockResolvedValueOnce(false);
  fireEvent.click(screen.getByRole("button", { name: /Codex default/ }));
  await waitFor(() => expect(mocks.confirmDialog).toHaveBeenCalled());
  expect(screen.getByLabelText("Profile name")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Save profile" }));
  await waitFor(() => expect(mocks.updatePreset).toHaveBeenCalledWith(expect.objectContaining({
    presetId: "preset-custom", expectedRevision: 2, name: "My audit", instructions: "Check the estimand. <workspace_context>",
    modules: ["research_structure", "empirical_audit", "paper_context", "paper_tools", "research_ledger", "evidence_inspector"],
  })));
});

it("restores a clone from its source after confirmation, keeping the name", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /My audit/ }));
  fireEvent.click(await screen.findByRole("button", { name: "Restore from Empirical audit" }));
  await waitFor(() => expect(mocks.updatePreset).toHaveBeenCalledWith(expect.objectContaining({
    presetId: "preset-custom", expectedRevision: 2, name: "My audit", instructions: "Identify the estimand.",
    modules: catalog().presets.find((preset) => preset.id === "empirical_audit")!.modules,
  })));
  expect(mocks.confirmDialog).toHaveBeenCalledWith(expect.stringContaining("Empirical audit"), expect.objectContaining({ destructive: true }));
});

it("shows the inheritance table with the winning scope and writes to global, workspace, and conversation scopes", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /Access & inheritance/ }));
  const table = await screen.findByRole("table");
  const rows = within(table).getAllByRole("row");
  const budgetRow = rows.find((row) => within(row).queryByText("Context budget"))!;
  const winning = within(budgetRow).getAllByRole("cell").filter((cell) => cell.dataset.winning);
  expect(winning).toHaveLength(1);
  expect(within(winning[0]).getByLabelText("Context budget (Global)")).toHaveValue("262144");

  fireEvent.click(within(table).getByRole("button", { name: "Inherit Command network for Workspace" }));
  await waitFor(() => expect(mocks.saveWorkspaceConfig).toHaveBeenCalledWith(expect.objectContaining({ workspaceId: "workspace", expectedRevision: 4, body: {} })));

  fireEvent.change(within(table).getByLabelText("Context budget (Global)"), { target: { value: "16384" } });
  await waitFor(() => expect(mocks.saveWorkspaceConfig).toHaveBeenCalledWith(expect.objectContaining({ workspaceId: null, expectedRevision: 2, body: { contextBudgetBytes: 16384 } })));

  fireEvent.click(within(table).getByRole("button", { name: "Override Access mode for This conversation" }));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ overrides: { commandNetwork: false, mode: "inspect" } })));

  fireEvent.click(within(table).getByRole("button", { name: "Inherit Command network for This conversation" }));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ overrides: {} })));

  expect(within(table).queryByLabelText("Native web search (Global)")).not.toBeInTheDocument();
  expect(screen.getByText(/Web search is not available in Workspace yet/)).toBeInTheDocument();
});

it("offers to switch an inspect conversation to Edit beside an execution module", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("tab", { name: "Tools & context" }));
  fireEvent.click(screen.getByRole("button", { name: "Switch this conversation to Edit" }));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ overrides: { commandNetwork: false, mode: "edit" } })));
});

it("renders the effective preview as labelled bands", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /Effective preview/ }));
  // The Instructions tab also previews these bands, so wait for the panel itself first.
  await screen.findByRole("heading", { name: "Effective preview", level: 2 });
  expect(screen.getByText("Pipeline instructions")).toBeInTheDocument();
  expect(screen.getByText("Preset instructions: Empirical audit")).toBeInTheDocument();
  expect(screen.getByText("Research execution · unavailable")).toBeInTheDocument();
  expect(screen.getByText("AGENTS.md")).toBeInTheDocument();
});

it("disables workspace-scope cells for an unfiled conversation", async () => {
  const unfiled = snapshot(); unfiled.workspace = null; unfiled.session.workspaceId = null;
  mocks.effectiveHarness.mockResolvedValue({ ...effective(), workspaceId: null });
  renderEditor({ snapshot: unfiled });
  await screen.findByRole("heading", { name: "Empirical audit" });
  expect(mocks.getWorkspaceConfig).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: /Access & inheritance/ }));
  const table = await screen.findByRole("table");
  expect(within(table).queryByRole("button", { name: "Override Access mode for Workspace" })).not.toBeInTheDocument();
  expect(within(table).getByRole("button", { name: "Override Access mode for Global" })).toBeInTheDocument();
});

it("shows the Codex default prompt layers honestly and keeps the built-in read-only", async () => {
  mocks.effectiveHarness.mockResolvedValue(effective("plain"));
  renderEditor({ snapshot: snapshot("plain") });
  await screen.findByRole("heading", { name: "Codex default", level: 2 });
  expect(screen.getByLabelText("Base prompt mode")).toBeDisabled();
  expect(screen.getByLabelText("Base prompt mode")).toHaveValue("default");
  expect(await screen.findByLabelText("Codex native base prompt")).toHaveTextContent("Cached base template");
  expect(screen.queryByRole("textbox", { name: "Supplemental instructions" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Save profile" })).not.toBeInTheDocument();
});

it("creates a blank profile for all Workspaces and selects it", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: "New profile" }));
  const dialog = await screen.findByRole("dialog");
  fireEvent.change(within(dialog).getByLabelText("Profile name"), { target: { value: "General writer" } });
  fireEvent.change(within(dialog).getByLabelText("Available in"), { target: { value: "global" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Create profile" }));
  await waitFor(() => expect(mocks.clonePreset).toHaveBeenCalledWith(expect.objectContaining({ workspaceId: null, sourcePresetId: "plain", name: "General writer" })));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ presetId: "preset-new" })));
});

it("searches profiles by name and description", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.change(screen.getByLabelText("Search agent profiles"), { target: { value: "Mine" } });
  expect(screen.getByRole("button", { name: /My audit/ })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Codex default/ })).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Search agent profiles"), { target: { value: "No such profile" } });
  expect(screen.getByText("No profiles match your search.")).toBeVisible();
});

it("discards a draft when navigating away and does not resurrect it on return", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /My audit/ }));
  await screen.findByLabelText("Profile name");
  fireEvent.change(await screen.findByLabelText("Supplemental instructions"), { target: { value: "Unsaved prompt" } });
  fireEvent.click(screen.getByRole("button", { name: /Codex default/ }));
  await screen.findByRole("heading", { name: "Codex default", level: 2 });
  fireEvent.click(screen.getByRole("button", { name: /My audit/ }));
  await screen.findByLabelText("Profile name");
  expect(screen.getByRole("textbox", { name: "Supplemental instructions" })).toHaveValue("Check the estimand.");
});

it("retains the creation dialog and name when saving fails", async () => {
  mocks.clonePreset.mockRejectedValueOnce(new Error("Could not save profile"));
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: "New profile" }));
  const dialog = await screen.findByRole("dialog");
  fireEvent.change(within(dialog).getByLabelText("Profile name"), { target: { value: "Keep this name" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Create profile" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Could not save profile");
  expect(within(dialog).getByLabelText("Profile name")).toHaveValue("Keep this name");
  expect(mocks.updateSession).not.toHaveBeenCalled();
});

it("saves a replacement separately from supplemental instructions and explicitly restores inheritance", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /My audit/ }));
  await screen.findByLabelText("Profile name");
  fireEvent.change(await screen.findByLabelText("Base prompt mode"), { target: { value: "replace" } });
  expect(screen.getByRole("button", { name: "Save profile" })).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Replacement base prompt"), { target: { value: "You are a writing partner." } });
  const saved = { ...custom, baseInstructions: "You are a writing partner.", revision: 3 };
  mocks.harnessCatalog.mockResolvedValue({ ...catalog(), presets: [...catalog().presets.filter(p => p.id !== custom.id), saved] });
  fireEvent.click(screen.getByRole("button", { name: "Save profile" }));
  await waitFor(() => expect(mocks.updatePreset).toHaveBeenCalledWith(expect.objectContaining({ basePrompt: { mode: "replace", text: "You are a writing partner." }, instructions: custom.instructions })));
  await waitFor(() => expect(screen.queryByText("Unsaved")).not.toBeInTheDocument());
  fireEvent.change(screen.getByLabelText("Base prompt mode"), { target: { value: "default" } });
  fireEvent.click(screen.getByRole("button", { name: "Save profile" }));
  await waitFor(() => expect(mocks.updatePreset).toHaveBeenLastCalledWith(expect.objectContaining({ basePrompt: { mode: "codexDefault" }, expectedRevision: 3 })));
});

it("lets a custom profile start from a viewable native template with its provenance", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /My audit/ }));
  await screen.findByLabelText("Profile name");
  fireEvent.click(await screen.findByText("View Codex default prompt"));
  expect(await screen.findByLabelText("Codex native base prompt")).toHaveTextContent("Cached base template");
  expect(screen.getByText(/does not establish which default/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Use this template as replacement" }));
  expect(screen.getByLabelText("Base prompt mode")).toHaveValue("replace");
  expect(screen.getByLabelText("Replacement base prompt")).toHaveValue("Cached base template");
  expect(screen.getByLabelText("Supplemental instructions")).toHaveValue(custom.instructions);
});

it("keeps replacement editing available when the cache cannot be read", async () => {
  mocks.nativePromptCatalog.mockRejectedValue(new Error("Cache unavailable"));
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /My audit/ }));
  await screen.findByLabelText("Profile name");
  fireEvent.click(await screen.findByText("View Codex default prompt"));
  expect(await screen.findByRole("alert")).toHaveTextContent("Cache unavailable");
  fireEvent.change(screen.getByLabelText("Base prompt mode"), { target: { value: "replace" } });
  fireEvent.change(screen.getByLabelText("Replacement base prompt"), { target: { value: "My base prompt" } });
  expect(screen.getByRole("button", { name: "Save profile" })).toBeEnabled();
});
