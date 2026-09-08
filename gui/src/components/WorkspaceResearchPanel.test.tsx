import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceResearchPanel from "./WorkspaceResearchPanel";
import type { ConversationSnapshot } from "../lib/workbenchTypes";
import { CATALOG } from "./harness-editor/harnessHelpers.test";

const mocks = vi.hoisted(() => ({ listPapers: vi.fn(), prepareReviewHandoff: vi.fn(), harnessCatalog: vi.fn(), effectiveHarness: vi.fn(), listRecipes: vi.fn(), listRecipeRuns: vi.fn() }));
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
  expect(screen.getByLabelText("Access mode")).toHaveValue("");
  expect(screen.queryByText("Clone as editable preset")).not.toBeInTheDocument();
  expect(screen.queryByText("Workspace defaults")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Manage agent profiles…" }));
  expect(onEditHarness).toHaveBeenCalledTimes(1);
});


it("requires an explicit paper choice for a project review without a conversation", async () => {
  mocks.listPapers.mockResolvedValue([
    { paper: { id: "paper-1", title: "First paper" }, revision: { id: "revision-1", contentHash: "abc123" } },
    { paper: { id: "paper-2", title: "Chosen paper" }, revision: { id: "revision-2", contentHash: "def456" } },
  ]);
  const handoff = { id: "handoff" };
  mocks.prepareReviewHandoff.mockResolvedValue(handoff);
  const review = vi.fn();
  render(<WorkspaceResearchPanel embedded workspace={{ id: "project", name: "Project" } as ConversationSnapshot["workspace"]}
    snapshot={null} initialTab="release" allowedTabs={["release"]} releaseView="review" onReviewHandoff={review}
    onSnapshot={vi.fn()} onClose={vi.fn()} onError={vi.fn()} />);
  await screen.findByRole("option", { name: "Chosen paper" });
  expect(screen.getByRole("button", { name: "Review this revision…" })).toBeDisabled();
  expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Paper to review"), { target: { value: "paper-2" } });
  fireEvent.click(screen.getByRole("button", { name: "Review this revision…" }));
  await vi.waitFor(() => expect(review).toHaveBeenCalledWith(handoff));
  expect(mocks.prepareReviewHandoff).toHaveBeenCalledWith(expect.objectContaining({ paperId: "paper-2", sessionId: null, workspaceId: "project" }));
});
