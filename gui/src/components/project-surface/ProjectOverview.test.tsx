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
import type { Workspace, WorkbenchSession } from "../../lib/workbenchTypes";

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

it("stays silent about sources that fail or have nothing to report", async () => {
  backend({});
  mount(home());
  await waitFor(() =>
    expect(screen.queryByText("Checking the state of the draft…")).toBeNull(),
  );
  const draft = screen.getByRole("region", { name: "State of the draft" });
  expect(within(draft).getByText("Minimum wage draft")).toBeVisible();
  for (const name of [
    "Out of sync",
    "Revision round",
    "Since you were last here",
    "Needs your decision",
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
        status: "complete",
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
        status: "complete",
        failed_steps: [],
        title: "",
      },
    ],
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
    workbench_binding_coverage: [
      {
        state: "stale",
        reasons: ["Declared execution inputs changed"],
        record: { body: { printed: "0.042" } },
      },
    ],
  });
  const data = home();
  data.ledger.staleClaims = ["claim"];
  const handlers = mount(data);

  const draft = await screen.findByRole("region", {
    name: "State of the draft",
  });
  expect(
    await within(draft).findByText(
      "2 review findings outstanding, 1 regressed",
    ),
  ).toBeVisible();
  expect(within(draft).getByText("Last review: Paper review")).toBeVisible();
  fireEvent.click(within(draft).getByRole("button", { name: "Open report →" }));
  expect(navigate).toHaveBeenCalledWith({ page: "history", runId: "run" });

  const sync = screen.getByRole("region", { name: "Out of sync" });
  expect(
    await within(sync).findByText(
      "1 number in the draft is out of date with its results",
    ),
  ).toBeVisible();
  fireEvent.click(
    within(sync).getByRole("button", { name: "Inspect evidence →" }),
  );
  expect(handlers.setTab).toHaveBeenCalledWith("evidence");
  for (const row of sync.querySelectorAll(".project-overview-row"))
    expect(within(row as HTMLElement).getAllByRole("button")).toHaveLength(1);
});

it("hands the paper to the assistant to draft a brief without sending anything", async () => {
  backend({});
  const handlers = mount(home());
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
  expect(
    screen.queryByRole("region", { name: "State of the draft" }),
  ).toBeNull();
  expect(screen.queryByRole("button", { name: "Check again" })).toBeNull();
  expect(
    screen.getByText(/this page will track\s+the state of the draft/),
  ).toBeVisible();
  const left = screen.getByRole("region", { name: "Where you left off" });
  expect(within(left).getByText("Latest thread")).toBeVisible();
  fireEvent.click(
    within(
      screen.getByRole("region", { name: "Other conversations" }),
    ).getByRole("button", { name: "Resume →" }),
  );
  expect(handlers.onResume).toHaveBeenCalledWith("Earlier idea");
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
    await screen.findByRole("button", { name: "Check GitHub →" }),
  );
  await waitFor(() =>
    expect(reportError).toHaveBeenCalledWith(
      "unavailable: workbench_repository_fetch",
    ),
  );
});
