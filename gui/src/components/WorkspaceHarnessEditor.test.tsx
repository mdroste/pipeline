import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceHarnessEditor from "./WorkspaceHarnessEditor";
import type { ConversationSnapshot, EffectiveHarness, HarnessCatalog, HarnessPreset } from "../lib/workbenchTypes";
import { CATALOG } from "./harness-editor/harnessHelpers.test";

const mocks = vi.hoisted(() => ({
  harnessCatalog: vi.fn(), effectiveHarness: vi.fn(), getWorkspaceConfig: vi.fn(), saveWorkspaceConfig: vi.fn(),
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
    presets: [
      builtIn("plain", "Plain conversation", "", []),
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
    instructionSections: [{ id: "preamble", label: "Workspace preamble (host-owned)", text: "preamble" }, { id: "preset", label: "Preset instructions: Empirical audit", text: "preset" }],
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
  expect(screen.getByText(/Built-in presets are read-only/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Use in this conversation" })).not.toBeInTheDocument();
  expect(screen.getByTestId("preset-summary")).toHaveTextContent("Adds 2 instruction packs, paper context, 3 tools, and 2 inspectors.");
  expect(screen.getAllByText("In use")).toHaveLength(1);
  expect(screen.getByText(/differs from the active native thread/)).toBeInTheDocument();
  expect(screen.getByText("Custom")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /My audit/ })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("tab", { name: "Modules" }));
  expect(screen.getByRole("switch", { name: "Research execution" })).toBeDisabled();
  expect(screen.getByText(/Needs Edit access mode/)).toBeInTheDocument();
});

it("clones a built-in through a named dialog and switches the conversation to the copy", async () => {
  const onSnapshot = vi.fn();
  renderEditor({ onSnapshot });
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: "Clone to edit" }));
  const dialog = screen.getByRole("dialog");
  const input = within(dialog).getByRole("textbox");
  fireEvent.change(input, { target: { value: "Field audit" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "OK" }));
  await waitFor(() => expect(mocks.clonePreset).toHaveBeenCalledWith(expect.objectContaining({ workspaceId: "workspace", sourcePresetId: "empirical_audit", name: "Field audit" })));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ presetId: "preset-new", expectedRevision: 3 })));
  expect(onSnapshot).toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("edits a custom preset as a draft, saves with the expected revision, and guards navigation", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /My audit/ }));
  const name = await screen.findByLabelText("Preset name");
  expect(name).toHaveValue("My audit");
  expect(screen.getByText("based on Empirical audit")).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Preset instructions"), { target: { value: "Check the estimand. <workspace_context>" } });
  expect(screen.getByText(/reserved for the host/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("tab", { name: "Modules" }));
  fireEvent.click(screen.getByRole("switch", { name: "Evidence inspector" }));
  fireEvent.click(screen.getByRole("button", { name: "Paper + ledger" }));
  expect(screen.getByText("Unsaved")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Use in this conversation" })).toBeDisabled();

  mocks.confirmDialog.mockResolvedValueOnce(false);
  fireEvent.click(screen.getByRole("button", { name: /Plain conversation/ }));
  await waitFor(() => expect(mocks.confirmDialog).toHaveBeenCalled());
  expect(screen.getByLabelText("Preset name")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Save preset" }));
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
  expect(screen.getByText(/no scope can enable it/)).toBeInTheDocument();
});

it("offers to switch an inspect conversation to Edit beside an execution module", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("tab", { name: "Modules" }));
  fireEvent.click(screen.getByRole("button", { name: "Switch this conversation to Edit" }));
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledWith(expect.objectContaining({ overrides: { commandNetwork: false, mode: "edit" } })));
});

it("renders the effective preview as labelled bands", async () => {
  renderEditor();
  await screen.findByRole("heading", { name: "Empirical audit" });
  fireEvent.click(screen.getByRole("button", { name: /Effective preview/ }));
  // The Instructions tab also previews these bands, so wait for the panel itself first.
  await screen.findByRole("heading", { name: "Effective preview", level: 2 });
  expect(screen.getByText("Workspace preamble (host-owned)")).toBeInTheDocument();
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
