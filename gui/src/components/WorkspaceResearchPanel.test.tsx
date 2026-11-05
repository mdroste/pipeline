import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceResearchPanel from "./WorkspaceResearchPanel";
import type { ConversationSnapshot } from "../lib/workbenchTypes";
import { CATALOG } from "./harness-editor/harnessHelpers.test";

const mocks = vi.hoisted(() => ({
  listPapers: vi.fn(),
  paperSearch: vi.fn(),
  prepareReviewHandoff: vi.fn(),
  harnessCatalog: vi.fn(),
  effectiveHarness: vi.fn(),
  listRecipes: vi.fn(),
  listRecipeRuns: vi.fn(),
  listSources: vi.fn(),
  listNotes: vi.fn(),
  researchLedger: vi.fn(),
  listExecutionProfiles: vi.fn(),
  listExecutions: vi.fn(),
}));
const projectMocks = vi.hoisted(() => ({ previewExecution: vi.fn() }));
vi.mock("../lib/workbench/harnessClient", () => ({ harnessClient: mocks }));
vi.mock("../lib/workbench/researchClient", () => ({ researchClient: mocks }));
vi.mock("../lib/workbench/executionClient", () => ({ executionClient: mocks }));
vi.mock("../lib/workbench/recipeClient", () => ({ recipeClient: mocks }));
vi.mock("../lib/workbench/releaseClient", () => ({ releaseClient: mocks }));
vi.mock("../lib/projectClient", () => ({ projectClient: projectMocks }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const preset = {
  id: "research_assistant",
  workspaceId: null,
  name: "Research assistant",
  description: "General",
  instructions: "Be precise.",
  modules: ["research_structure", "paper_context", "paper_tools"],
  builtIn: true,
  sourcePresetId: null,
  revision: 1,
};

function snapshot(): ConversationSnapshot {
  return {
    workspace: null,
    sequence: 1,
    activeBinding: null,
    turns: [],
    items: [],
    session: {
      id: "session",
      workspaceId: null,
      paperId: null,
      title: "Question",
      presetId: "research_assistant",
      overrides: {},
      draft: "",
      revision: 1,
      archivedAt: null,
      createdAt: "now",
      updatedAt: "now",
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listPapers.mockResolvedValue([]);
  mocks.listSources.mockResolvedValue([]);
  mocks.listNotes.mockResolvedValue([]);
  mocks.researchLedger.mockResolvedValue({
    claims: [],
    evidence: [],
    unsupportedAcceptedClaims: [],
    staleClaims: [],
  });
  mocks.listExecutionProfiles.mockResolvedValue([]);
  mocks.listExecutions.mockResolvedValue([]);
  mocks.harnessCatalog.mockResolvedValue({
    schemaVersion: 1,
    toolCatalogVersion: 4,
    modules: CATALOG,
    presets: [preset],
  });
  mocks.effectiveHarness.mockResolvedValue({
    schemaVersion: 1,
    sessionId: "session",
    workspaceId: null,
    preset,
    mode: "inspect",
    webSearch: false,
    commandNetwork: false,
    permissionProfile: "workbench-inspect",
    contextBudgetBytes: 65536,
    enabledModules: ["research_structure"],
    unavailableModules: ["paper_context", "paper_tools"],
    diagnostics: ["Put this conversation in a Workspace to read papers."],
    developerInstructions: "x",
    contextPreview: "",
    contextTruncated: false,
    dynamicTools: [],
    fingerprint: "f",
    valueSources: {},
  });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("shows a read-only harness summary and hands editing to the harness editor", async () => {
  const onEditHarness = vi.fn();
  render(
    <WorkspaceResearchPanel
      snapshot={snapshot()}
      onSnapshot={vi.fn()}
      onClose={vi.fn()}
      onError={vi.fn()}
      onEditHarness={onEditHarness}
    />,
  );
  expect(await screen.findByTestId("harness-summary")).toHaveTextContent(
    "Adds 1 research instruction set, paper context, and paper tools.",
  );
  expect(
    screen.getByText(
      "Inspect · no command network · 64 KiB · 1 of 3 modules active · workbench-inspect",
    ),
  ).toBeInTheDocument();
  expect(
    screen.getByText("Put this conversation in a Workspace to read papers."),
  ).toBeInTheDocument();
  expect(screen.getByLabelText("Access mode")).toHaveValue("");
  expect(
    screen.queryByText("Clone as editable preset"),
  ).not.toBeInTheDocument();
  expect(screen.queryByText("Workspace defaults")).not.toBeInTheDocument();
  fireEvent.click(
    screen.getByRole("button", { name: "Manage agent profiles…" }),
  );
  expect(onEditHarness).toHaveBeenCalledTimes(1);
});

it("requires an explicit paper choice for a project review without a conversation", async () => {
  mocks.listPapers.mockResolvedValue([
    {
      paper: { id: "paper-1", title: "First paper" },
      revision: { id: "revision-1", contentHash: "abc123" },
    },
    {
      paper: { id: "paper-2", title: "Chosen paper" },
      revision: { id: "revision-2", contentHash: "def456" },
    },
  ]);
  const handoff = { id: "handoff" };
  mocks.prepareReviewHandoff.mockResolvedValue(handoff);
  const review = vi.fn();
  render(
    <WorkspaceResearchPanel
      embedded
      workspace={
        { id: "project", name: "Project" } as ConversationSnapshot["workspace"]
      }
      snapshot={null}
      initialTab="release"
      allowedTabs={["release"]}
      releaseView="review"
      onReviewHandoff={review}
      onSnapshot={vi.fn()}
      onClose={vi.fn()}
      onError={vi.fn()}
    />,
  );
  await screen.findByRole("option", { name: "Chosen paper" });
  expect(
    screen.getByRole("button", { name: "Review this revision…" }),
  ).toBeDisabled();
  expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Paper to review"), {
    target: { value: "paper-2" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Review this revision…" }),
  );
  await vi.waitFor(() => expect(review).toHaveBeenCalledWith(handoff));
  expect(mocks.prepareReviewHandoff).toHaveBeenCalledWith(
    expect.objectContaining({
      paperId: "paper-2",
      sessionId: null,
      workspaceId: "project",
    }),
  );
});

function projectPanel(initialTab: "documents" | "results") {
  return (
    <WorkspaceResearchPanel
      workspace={
        {
          id: "project",
          name: "Project",
          root: "/research",
        } as ConversationSnapshot["workspace"]
      }
      snapshot={null}
      initialTab={initialTab}
      onSnapshot={vi.fn()}
      onClose={vi.fn()}
      onError={vi.fn()}
    />
  );
}

async function selectTab(name: string) {
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name }));
  });
}

it("loads only the selected research tab and retains its drafts across tabs", async () => {
  render(projectPanel("documents"));
  await vi.waitFor(() => expect(mocks.listSources).toHaveBeenCalledTimes(1));
  expect(mocks.listPapers).toHaveBeenCalledTimes(1);
  expect(mocks.listNotes).not.toHaveBeenCalled();
  expect(mocks.researchLedger).not.toHaveBeenCalled();
  expect(mocks.listExecutions).not.toHaveBeenCalled();
  expect(mocks.harnessCatalog).not.toHaveBeenCalled();

  await selectTab("Notes");
  await vi.waitFor(() => expect(mocks.listNotes).toHaveBeenCalledTimes(1));
  fireEvent.change(screen.getByPlaceholderText(/Question, assumption/), {
    target: { value: "An unsaved note" },
  });
  await selectTab("Evidence");
  await vi.waitFor(() => expect(mocks.researchLedger).toHaveBeenCalledTimes(1));
  fireEvent.change(screen.getByPlaceholderText("Record a claim for review…"), {
    target: { value: "An unsaved claim" },
  });
  await selectTab("Results");
  await vi.waitFor(() => expect(mocks.listExecutions).toHaveBeenCalledTimes(1));
  fireEvent.change(screen.getByLabelText("Argument vector JSON"), {
    target: { value: '["custom", "argument"]' },
  });
  await selectTab("Notes");
  expect(screen.getByPlaceholderText(/Question, assumption/)).toHaveValue(
    "An unsaved note",
  );
  await selectTab("Evidence");
  expect(screen.getByPlaceholderText("Record a claim for review…")).toHaveValue(
    "An unsaved claim",
  );
  await selectTab("Results");
  expect(screen.getByLabelText("Argument vector JSON")).toHaveValue(
    '["custom", "argument"]',
  );
  expect(mocks.listPapers).toHaveBeenCalledTimes(1);
  expect(mocks.listSources).toHaveBeenCalledTimes(1);
});

it("polls visible running results every 500 ms without overlapping requests", async () => {
  vi.useFakeTimers();
  const running = {
    id: "run",
    outcome: "running",
    adapter: "command",
    dependencyHash: "hash",
    command: [],
    inputManifest: {},
    validation: {},
    snapshotConsistency: "captured",
  };
  mocks.listExecutions.mockResolvedValue([running]);
  await act(async () => {
    render(projectPanel("results"));
  });
  expect(mocks.listExecutions).toHaveBeenCalledTimes(1);
  let finish!: (value: unknown[]) => void;
  mocks.listExecutions.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  await act(async () => {
    await vi.advanceTimersByTimeAsync(499);
  });
  expect(mocks.listExecutions).toHaveBeenCalledTimes(1);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1);
  });
  expect(mocks.listExecutions).toHaveBeenCalledTimes(2);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
  });
  expect(mocks.listExecutions).toHaveBeenCalledTimes(2);
  await act(async () => {
    finish([running]);
  });
  await selectTab("Notes");
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
  });
  expect(mocks.listExecutions).toHaveBeenCalledTimes(2);
});

it("retains a pending host execution preview when leaving and returning to Results", async () => {
  mocks.listExecutionProfiles.mockResolvedValue([
    { id: "profile", name: "Command", argv: ["fixture"], testStatus: "passed" },
  ]);
  projectMocks.previewExecution.mockResolvedValue({
    profileId: "profile",
    fingerprint: "fingerprint",
    boundary: "Fixture boundary",
    command: ["fixture"],
    cwd: "/research",
    inputs: [],
    launch: {},
    outputs: [],
  });
  render(projectPanel("results"));
  fireEvent.click(await screen.findByRole("button", { name: "Test" }));
  await screen.findByText("Fixture boundary");
  await selectTab("Notes");
  expect(screen.queryByText("Fixture boundary")).not.toBeInTheDocument();
  await selectTab("Results");
  expect(screen.getByText("Fixture boundary")).toBeInTheDocument();
  expect(projectMocks.previewExecution).toHaveBeenCalledExactlyOnceWith(
    "profile",
  );
});

it("switches execution adapters with batch templates while keeping typed arguments", async () => {
  render(projectPanel("results"));
  const adapter = await screen.findByLabelText("Execution adapter");
  const argv = screen.getByLabelText("Argument vector JSON");
  fireEvent.change(adapter, { target: { value: "stata" } });
  expect(argv).toHaveValue(JSON.stringify(["stata-mp", "-b", "do", "main.do"]));
  expect(
    screen.getByPlaceholderText("Expected output paths, one per line"),
  ).toHaveValue("main.log");
  expect(
    screen.getByText(/batch mode \(-b on macOS and Linux/),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Detect installed tools" }),
  ).toBeInTheDocument();
  fireEvent.change(adapter, { target: { value: "notebook" } });
  expect(JSON.parse((argv as HTMLTextAreaElement).value)[1]).toBe("nbconvert");
  expect(
    screen.getByPlaceholderText("Expected output paths, one per line"),
  ).toHaveValue("analysis.executed.ipynb");
  fireEvent.change(argv, { target: { value: '["jupyter", "custom"]' } });
  fireEvent.change(adapter, { target: { value: "python" } });
  expect(argv).toHaveValue('["jupyter", "custom"]');
  expect(
    screen.getByPlaceholderText("Expected output paths, one per line"),
  ).toHaveValue("");
});

const notesPanel = (embedded: boolean, onClose: () => void) => (
  <WorkspaceResearchPanel
    embedded={embedded}
    workspace={
      { id: "project", name: "Project" } as ConversationSnapshot["workspace"]
    }
    snapshot={null}
    initialTab="memory"
    allowedTabs={["memory"]}
    onSnapshot={vi.fn()}
    onClose={onClose}
    onError={vi.fn()}
  />
);

it("leaves Escape to the field on a project page", async () => {
  const onClose = vi.fn();
  render(notesPanel(true, onClose));
  fireEvent.keyDown(await screen.findByLabelText("Note text"), {
    key: "Escape",
  });
  expect(onClose).not.toHaveBeenCalled();
});

it("still closes the side panel on Escape", async () => {
  const onClose = vi.fn();
  render(notesPanel(false, onClose));
  fireEvent.keyDown(await screen.findByLabelText("Note text"), {
    key: "Escape",
  });
  expect(onClose).toHaveBeenCalledOnce();
});

it("clears search hits when another paper is chosen", async () => {
  const paper = (id: string) => ({
    paper: { id, title: `Paper ${id}`, role: "manuscript" },
    revision: {
      id: `revision-${id}`,
      contentHash: "abc123",
      inputKind: "pdf",
      extraction: { status: "complete" },
    },
  });
  mocks.listPapers.mockResolvedValue([paper("a"), paper("b")]);
  mocks.paperSearch.mockResolvedValue([
    { start: 1, end: 2, page: 1, line: 1, excerpt: "Found in the first paper" },
  ]);
  render(
    <WorkspaceResearchPanel
      embedded
      workspace={
        { id: "project", name: "Project" } as ConversationSnapshot["workspace"]
      }
      snapshot={null}
      initialTab="documents"
      allowedTabs={["documents"]}
      onSnapshot={vi.fn()}
      onClose={vi.fn()}
      onError={vi.fn()}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: /Paper a/ }));
  fireEvent.change(await screen.findByLabelText("Search this version"), {
    target: { value: "wages" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Search" }));
  await screen.findByText("Found in the first paper");

  fireEvent.click(screen.getByRole("button", { name: /Paper b/ }));
  await waitFor(() =>
    expect(screen.queryByText("Found in the first paper")).toBeNull(),
  );
});
