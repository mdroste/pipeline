import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceExchangePanel from "./WorkspaceExchangePanel";

const mocks = vi.hoisted(() => ({
  listExchangeConflicts: vi.fn(),
  listRecipeRuns: vi.fn(),
  listPapers: vi.fn(),
  listExecutions: vi.fn(),
  previewProjectExchange: vi.fn(),
  exportProjectExchange: vi.fn(),
  inspectProjectExchange: vi.fn(),
  previewProjectExchangeImport: vi.fn(),
  importProjectExchange: vi.fn(),
  resolveExchangeConflict: vi.fn(),
  storageReport: vi.fn(),
  pruneStorage: vi.fn(),
  restoreTrash: vi.fn(),
  emptyTrash: vi.fn(),
  draftWorkflow: vi.fn(),
  home: vi.fn(),
  invoke: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
}));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("../lib/projectClient", () => ({
  projectClient: { home: mocks.home },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.open,
  save: mocks.save,
}));

const run = (action: () => Promise<void>) => void action();
const props = () => ({
  workspaceId: "ws",
  sessionId: "s1",
  busy: false,
  onAction: run,
  onError: vi.fn(),
});

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listExchangeConflicts.mockResolvedValue([]);
  mocks.listRecipeRuns.mockResolvedValue([]);
  mocks.listPapers.mockResolvedValue([{ paper: { id: "p1" }, revision: null }]);
  mocks.listExecutions.mockResolvedValue([]);
  mocks.home.mockResolvedValue({
    tasks: [
      { id: "t1", body: { objective: "Done task", status: "completed" } },
      { id: "t2", body: { objective: "Open task", status: "open" } },
    ],
  });
});
afterEach(cleanup);

it("previews a package with the selected kinds and all papers, never conversations", async () => {
  mocks.previewProjectExchange.mockResolvedValue({
    objects: [{ kind: "task", id: "t", fingerprint: "f", title: "x" }],
    blobCount: 1,
    blobBytes: 2048,
    exclusions: [],
    externalReferences: [],
    limitations: ["Execution receipts are historical evidence"],
  });
  render(<WorkspaceExchangePanel {...props()} />);
  await screen.findByText("Done task");
  fireEvent.click(screen.getByLabelText("Acceptance journals"));
  fireEvent.click(screen.getByText("Preview package"));
  await waitFor(() => expect(mocks.previewProjectExchange).toHaveBeenCalled());
  const [, selection] = mocks.previewProjectExchange.mock.calls[0];
  expect(selection.recordKinds).toContain("application");
  expect(selection.recordKinds).toContain("task");
  expect(selection.paperIds).toEqual(["p1"]);
  expect(selection.includeNotes).toBe(true);
  expect(Object.keys(selection)).not.toContain("sessionIds");
  await screen.findByText("task: 1");
  expect(
    screen.getByText("Execution receipts are historical evidence"),
  ).toBeInTheDocument();
});

it("defaults an import to a new separate Workspace and records conflicts for review", async () => {
  mocks.open.mockResolvedValue("/tmp/coauthor.pwex");
  mocks.inspectProjectExchange.mockResolvedValue({
    path: "/tmp/coauthor.pwex",
    packageHash: "h",
    workspaceName: "Coauthor",
    sourceNamespace: "ns:ws",
    createdAt: "2026",
    workspaceRoot: null,
    counts: { task: 2 },
    blobCount: 0,
    blobBytes: 0,
    exclusions: [],
    externalReferences: 0,
    limitations: [],
    ownExport: false,
  });
  mocks.previewProjectExchangeImport.mockResolvedValue({
    targetWorkspaceId: null,
    new: 2,
    identical: 0,
    remapped: 0,
    conflicts: [],
    decisions: [],
    blobCount: 0,
    limitations: [],
  });
  mocks.importProjectExchange.mockResolvedValue({
    importId: "i",
    workspaceId: "ws2",
    new: 2,
    identical: 0,
    remapped: 0,
    conflicts: 0,
    blobs: 0,
    limitations: [],
  });
  render(<WorkspaceExchangePanel {...props()} />);
  fireEvent.click(await screen.findByText("Inspect package…"));
  await screen.findByText("Preview import");
  expect(screen.getByLabelText("New Workspace name")).toHaveValue(
    "Coauthor (imported)",
  );
  fireEvent.click(screen.getByText("Preview import"));
  await waitFor(() =>
    expect(mocks.previewProjectExchangeImport).toHaveBeenCalledWith(
      "/tmp/coauthor.pwex",
      { kind: "newWorkspace", name: "Coauthor (imported)", root: null },
    ),
  );
  fireEvent.click(screen.getByText("Import"));
  await waitFor(() => expect(mocks.importProjectExchange).toHaveBeenCalled());
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Imported 2 new object(s)",
  );
});

it("resolves an open conflict either way through the client", async () => {
  mocks.listExchangeConflicts.mockResolvedValue([
    {
      id: "c1",
      importId: "i",
      objectKind: "note",
      objectId: "n1",
      local: { body: "a" },
      imported: { body: "b" },
      state: "open",
      recordedAt: "t",
    },
  ]);
  mocks.resolveExchangeConflict.mockResolvedValue({});
  render(<WorkspaceExchangePanel {...props()} />);
  fireEvent.click(await screen.findByText("Take imported"));
  await waitFor(() =>
    expect(mocks.resolveExchangeConflict).toHaveBeenCalledWith(
      "ws",
      "c1",
      true,
    ),
  );
});

it("previews a prune before moving anything and only offers disposable categories", async () => {
  mocks.storageReport.mockResolvedValue({
    root: "/r",
    activeJobs: 0,
    note: "n",
    trash: [],
    categories: [
      {
        key: "blobs_referenced",
        label: "Referenced blobs",
        bytes: 10,
        entries: 1,
        disposable: false,
        note: "",
      },
      {
        key: "blobs_unreferenced",
        label: "Orphan blobs",
        bytes: 20,
        entries: 2,
        disposable: true,
        note: "",
      },
    ],
  });
  mocks.pruneStorage.mockResolvedValue({
    entries: [
      { category: "blobs_unreferenced", path: "/r/blobs/x", bytes: 20 },
    ],
    bytes: 20,
    applied: false,
    trashIds: [],
  });
  render(<WorkspaceExchangePanel {...props()} />);
  fireEvent.click(await screen.findByText("Inspect disk use"));
  await screen.findByText("Orphan blobs");
  expect(screen.queryByLabelText("Prune Referenced blobs")).toBeNull();
  expect(screen.getByText("Move to trash")).toBeDisabled();
  fireEvent.click(screen.getByLabelText("Prune Orphan blobs"));
  fireEvent.click(screen.getByText("Preview deletion"));
  await waitFor(() =>
    expect(mocks.pruneStorage).toHaveBeenCalledWith(
      ["blobs_unreferenced"],
      false,
    ),
  );
  await screen.findByText(/would move to trash. Nothing has been moved yet/);
});

it("drafts a workflow only from completed steps and saves it as a file", async () => {
  mocks.draftWorkflow.mockResolvedValue({
    name: "Research follow-up",
    canonicalJson: "{}",
    fingerprint: "sha256:abcdefabcdefabcdef",
    steps: [{ id: "done_task", label: "Done task", source: "task t1" }],
    unsupported: ["Task expects an execution"],
    notes: [],
  });
  mocks.save.mockResolvedValue("/tmp/draft.json");
  mocks.invoke.mockResolvedValue(undefined);
  render(<WorkspaceExchangePanel {...props()} />);
  await screen.findByText("Done task");
  expect(screen.queryByText("Open task")).toBeNull();
  expect(screen.getByText("Draft workflow")).toBeDisabled();
  fireEvent.click(screen.getByLabelText("Done task", { exact: false }));
  fireEvent.click(screen.getByText("Draft workflow"));
  await waitFor(() =>
    expect(mocks.draftWorkflow).toHaveBeenCalledWith({
      workspaceId: "ws",
      name: "Research follow-up",
      recipeRunIds: [],
      taskIds: ["t1"],
      theoryIds: [],
    }),
  );
  await screen.findByText("Task expects an execution");
  fireEvent.click(screen.getByText("Save draft…"));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("workbench_studio_export_file", {
      path: "/tmp/draft.json",
      content: "{}",
    }),
  );
});

it("invalidates changed prune categories and sends the reviewed token on apply", async () => {
  mocks.storageReport.mockResolvedValue({
    root: "/r",
    activeJobs: 0,
    note: "",
    trash: [],
    categories: [
      {
        key: "blobs_unreferenced",
        label: "Orphan blobs",
        bytes: 20,
        entries: 1,
        disposable: true,
        note: "",
      },
      {
        key: "job_scratch",
        label: "Job scratch",
        bytes: 30,
        entries: 1,
        disposable: true,
        note: "",
      },
    ],
  });
  mocks.pruneStorage.mockResolvedValue({
    entries: [],
    bytes: 0,
    applied: false,
    trashIds: [],
    previewToken: "reviewed",
  });
  render(<WorkspaceExchangePanel {...props()} />);
  fireEvent.click(await screen.findByText("Inspect disk use"));
  fireEvent.click(await screen.findByLabelText("Prune Orphan blobs"));
  fireEvent.click(screen.getByText("Preview deletion"));
  await waitFor(() => expect(screen.getByText("Move to trash")).toBeEnabled());
  fireEvent.click(screen.getByLabelText("Prune Job scratch"));
  expect(screen.getByText("Move to trash")).toBeDisabled();
  fireEvent.click(screen.getByText("Preview deletion"));
  await waitFor(() => expect(screen.getByText("Move to trash")).toBeEnabled());
  fireEvent.click(screen.getByText("Move to trash"));
  await waitFor(() =>
    expect(mocks.pruneStorage).toHaveBeenCalledWith(
      ["blobs_unreferenced", "job_scratch"],
      true,
      "reviewed",
    ),
  );
});

it("ignores a pending prune preview after its category selection changes", async () => {
  let finish!: (value: unknown) => void;
  mocks.storageReport.mockResolvedValue({
    root: "/r",
    activeJobs: 0,
    note: "",
    trash: [],
    categories: [
      {
        key: "blobs_unreferenced",
        label: "Orphan blobs",
        bytes: 20,
        entries: 1,
        disposable: true,
        note: "",
      },
    ],
  });
  mocks.pruneStorage.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  render(<WorkspaceExchangePanel {...props()} />);
  fireEvent.click(await screen.findByText("Inspect disk use"));
  fireEvent.click(await screen.findByLabelText("Prune Orphan blobs"));
  fireEvent.click(screen.getByText("Preview deletion"));
  await waitFor(() => expect(finish).toBeDefined());
  fireEvent.click(screen.getByLabelText("Prune Orphan blobs"));
  await act(async () =>
    finish({
      entries: [],
      bytes: 0,
      applied: false,
      trashIds: [],
      previewToken: "stale",
    }),
  );
  expect(screen.getByText("Move to trash")).toBeDisabled();
});
