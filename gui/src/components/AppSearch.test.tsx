import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import AppSearch from "./AppSearch";

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  listWorkspaces: vi.fn(),
  index: vi.fn(),
  listRuns: vi.fn(),
  listTasks: vi.fn(),
  task: vi.fn(),
  paperSearch: vi.fn(),
  fileSearch: vi.fn(),
  home: vi.fn(),
  links: vi.fn(),
  report: vi.fn(),
  workspace: null as null | {
    projectId: string;
    projectName: string;
    openView: ReturnType<typeof vi.fn>;
  },
}));
vi.mock("./nav-rail/navRailSlot", () => ({
  useNavRailSlot: () => ({ workspace: mocks.workspace }),
}));
vi.mock("../lib/projectClient", () => ({
  projectClient: { home: mocks.home },
}));
vi.mock("../lib/workbench/releaseClient", () => ({
  listProjectReviewLinks: mocks.links,
}));
vi.mock("../lib/router", () => ({ router: { navigate: mocks.navigate } }));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { listWorkspaces: mocks.listWorkspaces },
}));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: { index: mocks.index },
}));
vi.mock("../lib/reviewHistoryClient", () => ({
  reviewHistoryClient: { list: mocks.listRuns, report: mocks.report },
}));
vi.mock("../lib/taskClient", () => ({
  taskClient: { list: mocks.listTasks, get: mocks.task },
}));
vi.mock("../lib/workbench/researchClient", () => ({
  researchClient: { paperSearch: mocks.paperSearch },
}));
vi.mock("../lib/fileWorkspaceClient", () => ({
  workspaceFileAdapter: () => ({ search: mocks.fileSearch }),
}));

const props = () => ({
  onClose: vi.fn(),
  onOpenConversation: vi.fn(),
  onOpenRun: vi.fn(),
});

beforeEach(() => {
  mocks.task.mockResolvedValue({ scope: { workspaceId: null } });
  mocks.paperSearch.mockResolvedValue([]);
  mocks.fileSearch.mockResolvedValue({
    matches: [],
    searchedFiles: 0,
    skippedFiles: 0,
    truncated: false,
  });
  mocks.workspace = null;
  mocks.home.mockResolvedValue({ papers: [], notes: [], inventory: null });
  mocks.links.mockResolvedValue([]);
  mocks.report.mockResolvedValue({ step_outputs: [] });
  mocks.navigate.mockClear();
  mocks.listWorkspaces.mockResolvedValue({
    workspaces: [{ id: "trade", name: "Trade paper", updatedAt: "2026-10-30" }],
  });
  mocks.index.mockResolvedValue({
    items: [
      {
        session: {
          id: "chat",
          title: "Bartik shares",
          workspaceId: null,
          updatedAt: "2026-10-30",
        },
        projectName: null,
      },
    ],
    nextCursor: null,
  });
  mocks.listRuns.mockResolvedValue([
    {
      run_id: "run",
      input_name: "spillovers.pdf",
      input_path: "/trade/spillovers.pdf",
      profile_name: "Auto",
    },
  ]);
  mocks.listTasks.mockResolvedValue([{ id: "task", name: "Citation check" }]);
});

const search = async (text: string) => {
  const user = userEvent.setup();
  await user.type(
    await screen.findByRole("combobox", { name: "Search Pipeline" }),
    text,
  );
  return user;
};

it("finds projects, project views, and automations through the router", async () => {
  const handlers = props();
  render(<AppSearch {...handlers} />);
  await screen.findByRole("option", { name: /^Trade paperProject$/ });

  const user = await search("responses");
  await user.keyboard("{Enter}");
  expect(mocks.navigate).toHaveBeenCalledWith({
    page: "workspace",
    projectId: "trade",
    surface: "project",
    destination: "responses",
  });
  expect(handlers.onClose).toHaveBeenCalled();
});

it("opens conversations and reviews through their own paths", async () => {
  const handlers = props();
  const user = userEvent.setup();
  render(<AppSearch {...handlers} />);
  await user.click(await screen.findByRole("option", { name: /Bartik/ }));
  expect(handlers.onOpenConversation).toHaveBeenCalledWith("chat");

  await user.type(
    screen.getByRole("combobox", { name: "Search Pipeline" }),
    "spillovers",
  );
  await user.click(await screen.findByRole("option", { name: /spillovers/ }));
  expect(handlers.onOpenRun).toHaveBeenCalledWith("run");
});

it("keeps working when one source fails and marks results incomplete", async () => {
  mocks.listRuns.mockRejectedValue(new Error("offline"));
  render(<AppSearch {...props()} />);
  await search("citation");
  expect(
    await screen.findByRole("option", { name: /Citation check/ }),
  ).toBeVisible();
  await search("zzz");
  await waitFor(() =>
    expect(screen.getByText(/Results may be incomplete/)).toBeVisible(),
  );
});

it("ranks exact matches and lets each group expand", async () => {
  mocks.listTasks.mockResolvedValue(
    Array.from({ length: 12 }, (_, index) => ({
      id: String(index),
      name: index === 11 ? "Citation" : "Citation check " + index,
    })),
  );
  render(<AppSearch {...props()} />);
  const user = await search("Citation");
  expect(
    (await within(screen.getByRole("listbox")).findAllByRole("option"))[0],
  ).toHaveTextContent("CitationAutomation");
  expect(
    within(screen.getByRole("listbox")).getAllByRole("option"),
  ).toHaveLength(6);
  await user.click(
    screen.getByRole("button", { name: "Show all 12 automations" }),
  );
  expect(
    within(screen.getByRole("listbox")).getAllByRole("option"),
  ).toHaveLength(12);
});
it("keeps names available while retrying a failed source", async () => {
  mocks.listRuns.mockRejectedValueOnce(new Error("offline")).mockResolvedValue([
    {
      run_id: "retry",
      input_name: "Recovered review",
      input_path: "/trade/recovered.pdf",
      profile_name: "Paper",
    },
  ]);
  render(<AppSearch {...props()} />);
  const user = userEvent.setup();
  await screen.findByText(/Couldn't load Reviews/);
  expect(screen.getByRole("option", { name: /Citation check/ })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Retry search" }));
  expect(
    await screen.findByRole("option", { name: /Recovered review/ }),
  ).toBeVisible();
  expect(
    screen.queryByText(/Results may be incomplete/),
  ).not.toBeInTheDocument();
});

it("opens the exact saved document, note, file, and finding", async () => {
  mocks.home.mockResolvedValue({
    papers: [
      {
        paper: { id: "paper", title: "Trade manuscript" },
        revision: {
          id: "rev-7",
          contentHash: "abcdef12",
          entrypoint: "paper.tex",
        },
      },
    ],
    notes: [
      {
        id: "note-9",
        body: "Distinctive identification assumption",
        kind: "assumption",
        state: "accepted",
        revision: 2,
      },
    ],
    inventory: {
      body: {
        files: [{ path: "data/wages.csv", status: "present" }],
        capturedAt: "2026-11-24",
        complete: true,
      },
    },
  });
  mocks.report.mockResolvedValue({
    step_outputs: [
      {
        step_id: "findings",
        raw_text: JSON.stringify({
          issues: [
            {
              id: "f-3",
              title: "Missing controls",
              body: "Distinctive spillover explanation",
            },
          ],
        }),
      },
    ],
  });
  render(<AppSearch {...props()} />);
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("option", { name: /Trade manuscript/ }),
  );
  expect(mocks.navigate).toHaveBeenLastCalledWith(
    expect.objectContaining({
      projectId: "trade",
      destination: "documents",
      revisionId: "rev-7",
    }),
  );
  await user.selectOptions(screen.getByLabelText("Type"), "Notes");
  await user.type(
    screen.getByRole("combobox", { name: "Search Pipeline" }),
    "identification",
  );
  await user.click(
    await screen.findByRole("option", { name: /Distinctive identification/ }),
  );
  expect(mocks.navigate).toHaveBeenLastCalledWith(
    expect.objectContaining({ destination: "memory", noteId: "note-9" }),
  );
  await user.clear(screen.getByRole("combobox", { name: "Search Pipeline" }));
  await user.selectOptions(screen.getByLabelText("Type"), "Files");
  await user.click(await screen.findByRole("option", { name: /wages.csv/ }));
  expect(mocks.navigate).toHaveBeenLastCalledWith(
    expect.objectContaining({ destination: "files", file: "data/wages.csv" }),
  );
  await user.selectOptions(screen.getByLabelText("Type"), "Findings");
  await user.type(
    screen.getByRole("combobox", { name: "Search Pipeline" }),
    "spillover explanation",
  );
  await user.click(
    await screen.findByRole("option", { name: /Missing controls/ }),
  );
  expect(mocks.navigate).toHaveBeenLastCalledWith(
    expect.objectContaining({
      page: "history",
      runId: "run",
      findingId: "f-3",
    }),
  );
});

it("separates project scope from global actions and discloses unavailable content", async () => {
  mocks.workspace = {
    projectId: "trade",
    projectName: "Trade paper",
    openView: vi.fn(),
  };
  mocks.home.mockRejectedValue(new Error("offline"));
  render(<AppSearch {...props()} />);
  const user = userEvent.setup();
  expect(screen.getByLabelText("Scope")).toHaveValue("project");
  await screen.findByText(/Some sources are unavailable/);
  expect(
    screen.queryByRole("option", { name: /Bartik/ }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("option", { name: /Citation check/ }),
  ).not.toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText("Type"), "Notes");
  expect(
    await screen.findByText(/No matches in the available results/),
  ).toBeVisible();
  await user.selectOptions(screen.getByLabelText("Type"), "Actions");
  await user.click(
    screen.getByRole("option", { name: /New reviewOpen setup/ }),
  );
  expect(mocks.navigate).toHaveBeenLastCalledWith({ page: "main" });
  await user.selectOptions(screen.getByLabelText("Scope"), "all");
  await user.selectOptions(screen.getByLabelText("Type"), "Conversations");
  expect(await screen.findByRole("option", { name: /Bartik/ })).toBeVisible();
});

it("finds a document by its contents and opens the matching passage", async () => {
  mocks.home.mockResolvedValue({
    papers: [
      {
        paper: { id: "p1", title: "Manuscript" },
        revision: {
          id: "v1",
          contentHash: "hash",
          entrypoint: "draft.pdf",
          textReference: "text",
          captureComplete: true,
        },
      },
    ],
    notes: [],
    inventory: null,
  });
  mocks.paperSearch.mockResolvedValue([
    {
      revisionId: "v1",
      contentHash: "hash",
      start: 200,
      end: 220,
      page: 3,
      line: 12,
      excerpt: "Evidence for a distinctive mechanism",
    },
  ]);
  render(<AppSearch {...props()} />);
  const user = await search("distinctive mechanism");
  await user.click(
    await screen.findByRole("option", {
      name: /Manuscript.*distinctive mechanism.*Page 3/,
    }),
  );
  expect(mocks.navigate).toHaveBeenLastCalledWith(
    expect.objectContaining({
      projectId: "trade",
      revisionId: "v1",
      documentStart: 200,
      documentEnd: 220,
      documentPage: 3,
    }),
  );
});

it("uses passage results in place of duplicate document-name results", async () => {
  mocks.home.mockResolvedValue({
    papers: [
      {
        paper: { id: "p1", title: "Distinctive manuscript" },
        revision: {
          id: "v1",
          contentHash: "hash",
          entrypoint: "draft.pdf",
          textReference: "text",
          captureComplete: true,
        },
      },
    ],
    notes: [],
    inventory: null,
  });
  mocks.paperSearch.mockResolvedValue([
    {
      revisionId: "v1",
      contentHash: "hash",
      start: 200,
      end: 211,
      page: 3,
      line: 12,
      excerpt: "Distinctive evidence",
    },
  ]);
  render(<AppSearch {...props()} />);
  await search("Distinctive");
  await screen.findByText(/Contents searched: 1 document and/);
  expect(
    screen.getAllByRole("option", { name: /Distinctive manuscript/ }),
  ).toHaveLength(1);
});
it("finds live file contents outside the saved inventory and opens their line", async () => {
  mocks.listWorkspaces.mockResolvedValue({
    workspaces: [
      { id: "trade", name: "Trade", root: "/trade", updatedAt: "2026" },
    ],
  });
  mocks.fileSearch.mockResolvedValue({
    matches: [{ path: "new/code.do", line: 73, text: "distinctive mechanism" }],
    searchedFiles: 1,
    skippedFiles: 0,
    truncated: false,
  });
  render(<AppSearch {...props()} />);
  const user = await search("distinctive mechanism");
  await user.click(
    await screen.findByRole("option", {
      name: /code.do.*distinctive mechanism/,
    }),
  );
  expect(mocks.navigate).toHaveBeenLastCalledWith(
    expect.objectContaining({
      file: "new/code.do",
      fileLine: 73,
      projectId: "trade",
    }),
  );
});
it("includes project automations and preserves their project on opening", async () => {
  mocks.workspace = {
    projectId: "trade",
    projectName: "Trade",
    openView: vi.fn(),
  };
  mocks.task.mockResolvedValue({ scope: { workspaceId: "trade" } });
  render(<AppSearch {...props()} />);
  const user = await search("citation");
  await user.click(
    await screen.findByRole("option", { name: /Citation check/ }),
  );
  expect(mocks.navigate).toHaveBeenLastCalledWith({
    page: "tasks",
    taskId: "task",
    projectId: "trade",
  });
});
