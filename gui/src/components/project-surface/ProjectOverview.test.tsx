import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import ProjectOverview from "./ProjectOverview";
import type { ProjectHome } from "../../lib/projectClient";
import type {
  Workspace,
  WorkbenchSession,
} from "../../lib/workbench/sessionTypes";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const openExternal = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openExternal }));
const navigate = vi.hoisted(() => vi.fn());
vi.mock("../../lib/router", () => ({ router: { navigate } }));

const workspace = {
  id: "ws",
  name: "Minimum wage",
  root: "/research",
  revision: 1,
} as Workspace;
function home(): ProjectHome {
  return {
    settings: {
      id: "home",
      workspaceId: "ws",
      kind: "home",
      revision: 1,
      updatedAt: "",
      body: {
        manuscriptRevisionId: "rev",
        baselineExecutionId: null,
        briefNoteIds: [],
        excludedNoteIds: [],
        ignoredPaths: [],
        layout: "",
      },
    },
    notes: [],
    noteHistory: [],
    tasks: [],
    anchors: [],
    papers: [
      {
        paper: {
          id: "paper",
          workspaceId: "ws",
          title: "Minimum wage draft",
          role: "manuscript",
          currentRevisionId: "rev",
          createdAt: "",
          updatedAt: "",
        },
        revision: {
          id: "rev",
          paperId: "paper",
          inputKind: "file",
          entrypoint: "/research/paper.tex",
          dependencyManifest: {},
          contentHash: "hash",
          textReference: null,
          compiledArtifactId: null,
          extraction: {},
          captureComplete: true,
          capturedAt: "2026-09-20T10:00:00Z",
        },
      },
    ],
    executions: [],
    ledger: {
      claims: [],
      evidence: [],
      staleClaims: [],
      unsupportedAcceptedClaims: [],
    },
    inventory: null,
    changes: [],
    applications: [],
    contextPreview: "",
    workingCopyStatus: "",
    workingCopyChanged: 0,
    fileAcceptance: true,
  };
}
const session = (id: string, updatedAt: string): WorkbenchSession => ({
  id,
  workspaceId: "ws",
  paperId: null,
  title: id,
  presetId: null,
  overrides: {},
  draft: "",
  revision: 1,
  archivedAt: null,
  createdAt: updatedAt,
  updatedAt,
});

/** Answers each backend command from a map; anything else fails like a missing source. */
function backend(commands: Record<string, unknown>) {
  invoke.mockImplementation((command: string, args?: { kind?: string }) => {
    const key =
      command === "workbench_studio_records" ||
      command === "workbench_desk_records"
        ? `${command}:${args?.kind}`
        : command;
    return key in commands
      ? Promise.resolve(commands[key])
      : Promise.reject(new Error(`unavailable: ${key}`));
  });
}
function mount(
  data: ProjectHome,
  patch: Partial<Parameters<typeof ProjectOverview>[0]> = {},
) {
  const handlers = {
    setTab: vi.fn(),
    onAsk: vi.fn(),
    onResume: vi.fn(),
    onStartConversation: vi.fn(),
    onOpenDocument: vi.fn(),
  };
  render(
    <ProjectOverview
      workspaceId="ws"
      data={data}
      workspace={workspace}
      act={vi.fn()}
      run={vi.fn()}
      saveSettings={vi.fn()}
      reportError={vi.fn()}
      openAnnotation={vi.fn()}
      taskId=""
      setTaskId={vi.fn()}
      sessions={[]}
      onImportPaper={vi.fn()}
      onAttachFolder={vi.fn()}
      {...handlers}
      {...patch}
    />,
  );
  return handlers;
}

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
});
afterEach(cleanup);

it("opens the named document in the requested mode and targets a specific task", async () => {
  backend({});
  const data = home();
  data.tasks = [
    {
      id: "task-42",
      revision: 2,
      body: { objective: "Verify clustering", status: "open" },
    },
  ] as never;
  const onOpenTarget = vi.fn();
  const handlers = mount(data, { onOpenTarget });
  fireEvent.click(
    screen.getByRole("button", { name: "Edit Minimum wage draft" }),
  );
  expect(handlers.onOpenDocument).toHaveBeenLastCalledWith("rev", "edit");
  fireEvent.click(
    screen.getByRole("button", { name: "Read Minimum wage draft" }),
  );
  expect(handlers.onOpenDocument).toHaveBeenLastCalledWith("rev", undefined);
  fireEvent.click(
    screen.getByRole("button", { name: "Compare Minimum wage draft" }),
  );
  expect(handlers.onOpenDocument).toHaveBeenLastCalledWith("rev", "compare");
  fireEvent.click(screen.getByRole("button", { name: "Open task →" }));
  expect(onOpenTarget).toHaveBeenCalledWith(
    expect.objectContaining({ kind: "tab", tab: "tasks", targetId: "task-42" }),
  );
});

it("stays silent about sources that fail or have nothing to report", async () => {
  backend({});
  mount(home());
  await waitFor(() =>
    expect(screen.queryByText("Checking the state of the draft…")).toBeNull(),
  );
  const draft = screen.getByRole("region", { name: "Working set" });
  expect(within(draft).getByText("Minimum wage draft")).toBeVisible();
  for (const name of [
    "Out of sync",
    "Revision round",
    "Since you were last here",
    "Needs you",
    "Other conversations",
  ])
    expect(screen.queryByRole("region", { name })).toBeNull();
});

it("shows the draft's review state and what is out of sync, one action per row", async () => {
  backend({
    list_runs: [
      {
        run_id: "run",
        created: "2026-09-25T10:00:00Z",
        duration_secs: 60,
        input_path: "/research/paper.tex",
        input_name: "paper.tex",
        profile_name: "Paper review",
        status: "done",
        failed_steps: [],
        title: "Draft 7",
      },
      {
        run_id: "elsewhere",
        created: "2026-09-26T10:00:00Z",
        duration_secs: 60,
        input_path: "/other/paper.tex",
        input_name: "paper.tex",
        profile_name: "Other",
        status: "done",
        failed_steps: [],
        title: "",
      },
    ],
    workbench_project_review_links: [],
    list_projects: {
      projects: [{ id: "collection", name: "Reviews", run_ids: ["run"] }],
      warnings: [],
    },
    sync_project_issue_ledger: {
      issues: [
        { id: "1", title: "Pre-trends", severity: "high", status: "regressed" },
        { id: "2", title: "Clustering", severity: "low", status: "open" },
      ],
    },
  });
  const data = home();
  data.ledger.staleClaims = ["claim"];
  mount(data);

  const attention = await screen.findByRole("region", { name: "Attention" });
  expect(
    await within(attention).findByText(
      "2 review findings outstanding, 1 regressed",
    ),
  ).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Builds & reviews →" }));
  const dialog = screen.getByRole("dialog", { name: "Builds & reviews" });
  fireEvent.click(
    within(dialog).getByRole("button", { name: "Inspect latest report →" }),
  );
  expect(navigate).toHaveBeenCalledWith({ page: "history", runId: "run" });
  expect(invoke).not.toHaveBeenCalledWith(
    "workbench_binding_coverage",
    expect.anything(),
  );
});

it("hands the paper to the assistant to draft a brief without sending anything", async () => {
  backend({});
  const handlers = mount(home());
  fireEvent.click(screen.getByRole("button", { name: "Context" }));
  fireEvent.click(
    screen.getByRole("button", { name: "Draft a brief from the paper" }),
  );
  expect(handlers.onAsk).toHaveBeenCalledWith(
    expect.stringContaining("propose a short research brief"),
    [
      {
        role: "main",
        object: { kind: "paper", id: "rev", revision: "hash" },
      },
    ],
  );
});

it("keeps a conversation-only project to its conversations and loads nothing", () => {
  backend({});
  const data = home();
  data.papers = [];
  data.settings.body.manuscriptRevisionId = null;
  const handlers = mount(data, {
    workspace: { ...workspace, root: null },
    sessions: [
      session("Earlier idea", "2026-09-01"),
      session("Latest thread", "2026-09-03"),
    ],
  });
  expect(invoke).not.toHaveBeenCalled();
  expect(screen.queryByRole("region", { name: "Project activity" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Check again" })).toBeNull();
  const resume = screen.getByRole("region", { name: "Resume work" });
  expect(within(resume).getByText("Latest thread")).toBeVisible();
  fireEvent.click(
    within(resume).getByRole("button", { name: "Resume conversation →" }),
  );
  expect(handlers.onResume).toHaveBeenCalledWith("Latest thread");
  fireEvent.click(screen.getByRole("button", { name: "Recent work →" }));
  expect(screen.getByRole("dialog", { name: "Recent work" })).toHaveTextContent(
    "Earlier idea",
  );
});

it("shows an optional target date only when one is set", () => {
  backend({});
  const data = home();
  data.settings.body.targetDate = "2099-01-15";
  data.settings.body.targetLabel = "Resubmission";
  mount(data);
  expect(
    screen.getByText(/^Resubmission · .*2099.* · in \d+ days$/),
  ).toBeVisible();
});

it("checks the Git remote only when asked and shows what a coauthor pushed", async () => {
  const repository = {
    branch: "main",
    head: "1111111",
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    fetchedAt: null,
    changed: 1,
    untracked: 0,
    conflicts: 0,
    changedPaths: ["paper/main.tex"],
    remote: {
      name: "origin",
      host: "github.com",
      owner: "mdroste",
      repo: "minwage",
      webUrl: "https://github.com/mdroste/minwage",
    },
    commits: [],
    incoming: [],
  };
  backend({
    workbench_repository_status: repository,
    workbench_repository_fetch: {
      ...repository,
      behind: 1,
      fetchedAt: new Date().toISOString(),
      incoming: [
        {
          sha: "2",
          author: "Sam Lee",
          date: "2026-09-30T10:00:00Z",
          subject: "Add commuting-zone clusters",
        },
      ],
    },
  });
  mount(home());
  fireEvent.click(
    await screen.findByRole("button", { name: "Repository: main" }),
  );
  const block = await screen.findByRole("region", { name: "Repository" });
  expect(within(block).getByText("1 uncommitted change")).toBeVisible();
  expect(invoke).not.toHaveBeenCalledWith(
    "workbench_repository_fetch",
    expect.anything(),
  );
  fireEvent.click(
    within(block).getByRole("button", { name: "Open on GitHub →" }),
  );
  await waitFor(() =>
    expect(openExternal).toHaveBeenCalledWith(
      "https://github.com/mdroste/minwage",
    ),
  );
  fireEvent.click(
    await within(block).findByRole("button", { name: "Check GitHub →" }),
  );
  expect(
    await within(block).findByText("1 new commit on GitHub by Sam Lee"),
  ).toBeVisible();
  expect(invoke).toHaveBeenCalledWith("workbench_repository_fetch", {
    workspaceId: "ws",
  });
});

it("reports a remote that cannot be reached instead of failing silently", async () => {
  backend({
    workbench_repository_status: {
      branch: "main",
      head: "1",
      upstream: "origin/main",
      ahead: 0,
      behind: 0,
      fetchedAt: null,
      changed: 0,
      untracked: 0,
      conflicts: 0,
      changedPaths: [],
      remote: {
        name: "origin",
        host: "github.com",
        owner: "o",
        repo: "r",
        webUrl: "https://github.com/o/r",
      },
      commits: [],
      incoming: [],
    },
  });
  const reportError = vi.fn();
  mount(home(), { reportError });
  fireEvent.click(
    await screen.findByRole("button", { name: "Repository: main" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Check GitHub →" }),
  );
  await waitFor(() =>
    expect(reportError).toHaveBeenCalledWith(
      "unavailable: workbench_repository_fetch",
    ),
  );
});

it("keeps unknown review status distinct from zero findings", async () => {
  backend({});
  mount(home());
  expect(screen.queryByText("0 outstanding")).toBeNull();
});

it("keeps document rows beyond the preview reachable, including the full library", async () => {
  backend({});
  const data = home();
  const original = data.papers[0];
  data.papers = Array.from({ length: 8 }, (_, i) => ({
    paper: { ...original.paper, id: `paper-${i}`, title: `Document ${i + 1}` },
    revision: { ...original.revision!, id: `revision-${i}` },
  }));
  const handlers = mount(data);
  expect(screen.getByText("Document 3")).toBeVisible();
  expect(screen.queryByText("Document 4")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "All documents →" }));
  expect(handlers.setTab).toHaveBeenCalledWith("documents");
});

it("preserves a new context note across closing", async () => {
  backend({
    list_runs: [],
    list_projects: { projects: [], warnings: [] },
    workbench_project_review_links: [],
  });
  mount(home());
  const trigger = screen.getByRole("button", { name: "Context" });
  trigger.focus();
  fireEvent.click(trigger);
  fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
  fireEvent.change(screen.getByRole("textbox", { name: "New note" }), {
    target: { value: "How does credit affect investment?" },
  });
  fireEvent.keyDown(document, { key: "Escape" });
  await waitFor(() => expect(trigger).toHaveFocus());
  fireEvent.click(trigger);
  expect(screen.getByRole("textbox", { name: "New note" })).toHaveValue(
    "How does credit affect investment?",
  );
});

it("switches brief to notes in a single modal and retains note drafts", async () => {
  backend({});
  const data = home();
  data.notes = [
    {
      id: "note",
      workspaceId: "ws",
      paperId: null,
      kind: "question",
      body: "What drives investment?",
      state: "accepted",
      origin: "user",
      pinned: true,
      revision: 1,
      createdAt: "",
      updatedAt: "",
    },
  ];
  data.notes.push({ ...data.notes[0], id: "note-2", body: "Another question" });
  mount(data);
  const trigger = screen.getByRole("button", { name: "Context" });
  trigger.focus();
  fireEvent.click(trigger);
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  expect(trigger.closest("[inert]")).not.toBeNull();
  fireEvent.click(screen.getAllByRole("button", { name: "Edit note" })[0]);
  fireEvent.change(screen.getByRole("textbox", { name: "Edit note" }), {
    target: { value: "Unsaved question wording" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Suggested" }));
  expect(screen.queryByRole("textbox", { name: "Edit note" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Brief" }));
  expect(screen.getByRole("textbox", { name: "Edit note" })).toHaveValue(
    "Unsaved question wording",
  );
  fireEvent.keyDown(document, { key: "Escape" });
  await waitFor(() => expect(trigger).toHaveFocus());
  expect(trigger.closest("[inert]")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Context" }));
  expect(screen.getByRole("textbox", { name: "Edit note" })).toHaveValue(
    "Unsaved question wording",
  );
});

it("keeps portal controls disabled while a project write is pending and exposes save errors", async () => {
  backend({});
  mount(home(), { busy: true, error: "The project could not be saved." });
  fireEvent.click(screen.getByRole("button", { name: "Options" }));
  const dialog = screen.getByRole("dialog", { name: "Project options" });
  expect(
    within(dialog).getByRole("button", { name: "Project settings" }),
  ).toBeDisabled();
  expect(
    within(dialog).getByRole("button", { name: "Close panel" }),
  ).toBeEnabled();
  expect(within(dialog).getByRole("alert")).toHaveTextContent(
    "The project could not be saved.",
  );
});

it("keeps import and attach-folder actions available through Add", () => {
  backend({});
  const data = home();
  data.papers = [];
  const onImportPaper = vi.fn(),
    onAttachFolder = vi.fn();
  mount(data, {
    workspace: { ...workspace, root: null },
    onImportPaper,
    onAttachFolder,
  });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  fireEvent.click(screen.getByRole("button", { name: "Import LaTeX folder" }));
  expect(onImportPaper).toHaveBeenCalledWith(true);
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  fireEvent.click(screen.getByRole("button", { name: "Attach folder" }));
  expect(onAttachFolder).toHaveBeenCalledOnce();
});
