import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceResearchPanel from "./WorkspaceResearchPanel";
import type { ConversationSnapshot } from "../lib/workbenchTypes";
import { CATALOG } from "./harness-editor/harnessHelpers.test";

const mocks = vi.hoisted(() => ({ harnessCatalog: vi.fn(), effectiveHarness: vi.fn(), listRecipes: vi.fn(), listRecipeRuns: vi.fn() }));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("../lib/projectClient", () => ({ projectClient: {} }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const preset = { id: "research_assistant", workspaceId: null, name: "Research assistant", description: "General", instructions: "Be precise.", modules: ["research_structure", "paper_context", "paper_tools"], builtIn: true, sourcePresetId: null, revision: 1 };

function snapshot(): ConversationSnapshot {
  return {
    workspace: null, sequence: 1, activeBinding: null, turns: [], items: [],
    session: { id: "session", workspaceId: null, paperId: null, title: "Question", presetId: "research_assistant", overrides: {}, draft: "", revision: 1, archivedAt: null, createdAt: "now", updatedAt: "now" },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.harnessCatalog.mockResolvedValue({ schemaVersion: 1, toolCatalogVersion: 4, modules: CATALOG, presets: [preset] });
  mocks.effectiveHarness.mockResolvedValue({
    schemaVersion: 1, sessionId: "session", workspaceId: null, preset, mode: "inspect", webSearch: false, commandNetwork: false, permissionProfile: "workbench-inspect",
    contextBudgetBytes: 65536, enabledModules: ["research_structure"], unavailableModules: ["paper_context", "paper_tools"], diagnostics: ["Put this conversation in a Workspace to read papers."],
    developerInstructions: "x", contextPreview: "", contextTruncated: false, dynamicTools: [], fingerprint: "f", valueSources: {},
  });
});
afterEach(cleanup);

it("shows a read-only harness summary and hands editing to the harness editor", async () => {
  const onEditHarness = vi.fn();
  render(<WorkspaceResearchPanel snapshot={snapshot()} onSnapshot={vi.fn()} onClose={vi.fn()} onError={vi.fn()} onEditHarness={onEditHarness} />);
  expect(await screen.findByTestId("harness-summary")).toHaveTextContent("Adds 1 instruction pack, paper context, and paper tools.");
  expect(screen.getByText("Inspect · no command network · 64 KiB · 1 of 3 modules active · workbench-inspect")).toBeInTheDocument();
  expect(screen.getByText("Put this conversation in a Workspace to read papers.")).toBeInTheDocument();
  expect(screen.getByLabelText("Access mode")).toHaveValue("inspect");
  expect(screen.queryByText("Clone as editable preset")).not.toBeInTheDocument();
  expect(screen.queryByText("Workspace defaults")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Edit harness…" }));
  expect(onEditHarness).toHaveBeenCalledTimes(1);
});
