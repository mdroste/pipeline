import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceProjectSurface from "./WorkspaceProjectSurface";
import type { ProjectHome } from "../lib/projectClient";
import type { WorkbenchSession } from "../lib/workbenchTypes";
const mocks = vi.hoisted(() => ({
  home: vi.fn(),
  mutate: vi.fn(),
  getWorkspace: vi.fn(),
}));
vi.mock("../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../lib/projectClient")>()),
  projectClient: mocks,
}));
vi.mock("../lib/workbenchClient", () => ({
  workbenchClient: { getWorkspace: mocks.getWorkspace },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
function home(): ProjectHome {
  return {
    settings: {
      id: "home",
      workspaceId: "workspace",
      kind: "home",
      revision: 4,
      updatedAt: "now",
      body: {
        manuscriptRevisionId: null,
        baselineExecutionId: null,
        briefNoteIds: [],
        excludedNoteIds: [],
        ignoredPaths: [],
        layout: "reading",
      },
    },
    notes: [],
    noteHistory: [],
    tasks: [],
    anchors: [],
    papers: [],
    executions: [],
    ledger: {
      claims: [],
      evidence: [],
      staleClaims: [],
      unsupportedAcceptedClaims: [],
    },
    inventory: {
      id: "inventory",
      workspaceId: "workspace",
      kind: "inventory",
      revision: 1,
      updatedAt: "now",
      body: {
        rootIdentity: "root",
        capturedAt: "now",
        complete: true,
        warnings: [],
        files: [
          {
            path: "paper.tex",
            hash: "hash",
            size: 20,
            executable: false,
            status: "current",
          },
        ],
      },
    },
    changes: [],
    applications: [],
    workingCopyStatus: "Refresh files",
    fileAcceptance: true,
    contextPreview: "Reference results run: not selected.",
  };
}
const openTask = {
  id: "task",
  workspaceId: "workspace",
  kind: "task",
  revision: 1,
  updatedAt: "now",
  body: {
    objective: "Revise proof",
    anchorId: null,
    expectedOutputs: [],
    expectedChecks: [],
    status: "open",
  },
};
beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  mocks.home.mockResolvedValue(home());
  mocks.getWorkspace.mockResolvedValue({
    id: "workspace",
    name: "Local research",
    root: "/research",
    revision: 1,
  });
  mocks.mutate.mockResolvedValue({ id: "task", revision: 1, body: {} });
});
afterEach(cleanup);
const mount = async (conversation = vi.fn()) => {
  render(
    <WorkspaceProjectSurface
      workspaceId="workspace"
      onConversation={conversation}
      onWorkspaceChanged={vi.fn()}
    />,
  );
  await screen.findByRole("heading", { name: "Local research" });
};

it("preserves legacy desk preferences without making the project content own assistant sizing", async () => {
  localStorage.setItem(
    "pipeline.desk.workspace",
    JSON.stringify({ assistantWidth: 55 }),
  );
  await mount();
  expect(
    screen.queryByLabelText("Assistant pane width"),
  ).not.toBeInTheDocument();
  expect(
    JSON.parse(localStorage.getItem("pipeline.desk.workspace")!).assistantWidth,
  ).toBe(55);
});

it("opens signed-out project state without a model call or choosing reference results", async () => {
  const conversation = vi.fn();
  await mount(conversation);
  fireEvent.click(screen.getByRole("button", { name: "Project settings" }));
  expect(screen.getByLabelText("Reference results run")).toHaveValue("");
  expect(screen.getByLabelText("Current version of the paper")).toHaveValue("");
  expect(screen.getByText("research")).toBeInTheDocument();
  expect(mocks.mutate).not.toHaveBeenCalled();
  expect(conversation).not.toHaveBeenCalled();
});
it("uses plain-language section names instead of provenance vocabulary", async () => {
  await mount();
  for (const label of [
    "Overview",
    "Library",
    "Analyze",
    "Write",
    "Automate",
    "Activity",
  ])
    expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
  expect(screen.queryByText(/accepted research state/i)).toBeNull();
  expect(screen.queryByText(/computational baseline/i)).toBeNull();
  expect(screen.queryByText(/isolated task copy/i)).toBeNull();
});
it("records the task and user-specified outputs and checks", async () => {
  await mount();
  fireEvent.click(screen.getByRole("button", { name: "Automate" }));
  fireEvent.change(screen.getByLabelText("New action item"), {
    target: { value: "Check the envelope condition" },
  });
  fireEvent.change(screen.getByLabelText("Intended outputs"), {
    target: { value: "model.tex\nproof.md" },
  });
  fireEvent.change(screen.getByLabelText("Checks to run"), {
    target: { value: "Finite difference\nCompile manuscript" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add action item" }));
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenCalledWith("workspace", {
      action: "createTask",
      objective: "Check the envelope condition",
      anchorId: null,
      expectedOutputs: ["model.tex", "proof.md"],
      expectedChecks: ["Finite difference", "Compile manuscript"],
    }),
  );
});
it("starts an edit from a task and copies only the explicitly selected files", async () => {
  const data = home();
  data.tasks = [openTask];
  mocks.home.mockResolvedValue(data);
  await mount();
  fireEvent.click(screen.getByRole("button", { name: "Automate" }));
  fireEvent.click(screen.getByRole("button", { name: "Start an edit" }));
  expect(screen.getByLabelText("Action item for this edit")).toHaveValue(
    "task",
  );
  expect(
    screen.getByRole("button", { name: "Copy 0 files and start" }),
  ).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Select shown files" }));
  fireEvent.click(
    screen.getByRole("button", { name: "Copy 1 file and start" }),
  );
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenCalledWith("workspace", {
      action: "checkpoint",
      taskId: "task",
      paths: ["paper.tex"],
      backend: "copy",
    }),
  );
});
it("explains that a folder is needed before an edit can start", async () => {
  mocks.getWorkspace.mockResolvedValue({
    id: "workspace",
    name: "Local research",
    root: null,
    revision: 1,
  });
  await mount();
  expect(screen.getByText("No folder attached")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Write" }));
  fireEvent.change(screen.getByLabelText("Project view"), {
    target: { value: "edits" },
  });
  expect(
    screen.getByText(/Attach a folder on the Overview tab first/),
  ).toBeInTheDocument();
  expect(screen.queryByLabelText("Action item for this edit")).toBeNull();
});
it("opens a research conversation only when explicitly requested", async () => {
  const conversation = vi.fn().mockResolvedValue(undefined);
  await mount(conversation);
  fireEvent.click(screen.getByRole("button", { name: "Start conversation →" }));
  await waitFor(() => expect(conversation).toHaveBeenCalledWith(""));
});

it("keeps an unsaved project note when changing project destinations", async () => {
  await mount();
  expect(screen.getByLabelText("New note")).not.toBeVisible();
  expect(
    screen.queryByLabelText("Current version of the paper"),
  ).not.toBeInTheDocument();
  fireEvent.click(
    screen.getByText("Project notes", { selector: "summary", exact: false }),
  );
  const draft = screen.getByLabelText("New note");
  fireEvent.change(draft, {
    target: { value: "Retain the aggregation assumptions" },
  });
  fireEvent.click(
    screen.getByText("Project notes", { selector: "summary", exact: false }),
  );
  expect(draft).not.toBeVisible();
  fireEvent.click(
    screen.getByText("Project notes", { selector: "summary", exact: false }),
  );
  expect(draft).toHaveValue("Retain the aggregation assumptions");
  fireEvent.click(screen.getByRole("button", { name: "Automate" }));
  expect(draft).not.toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Overview" }));
  expect(draft).toBeVisible();
  expect(draft).toHaveValue("Retain the aggregation assumptions");
  expect(mocks.mutate).not.toHaveBeenCalled();
});

it("resumes the latest active conversation in this project without generating a prompt", async () => {
  const resume = vi.fn().mockResolvedValue(undefined);
  const conversation = vi.fn();
  const session = (
    id: string,
    updatedAt: string,
    patch: Partial<WorkbenchSession> = {},
  ): WorkbenchSession => ({
    id,
    workspaceId: "workspace",
    title: id,
    updatedAt,
    createdAt: updatedAt,
    paperId: null,
    presetId: null,
    overrides: {},
    draft: "Saved draft",
    revision: 1,
    archivedAt: null,
    ...patch,
  });
  render(
    <WorkspaceProjectSurface
      workspaceId="workspace"
      onWorkspaceChanged={vi.fn()}
      onConversation={conversation}
      onResumeSession={resume}
      sessions={[
        session("old", "2026-09-01"),
        session("resume-this", "2026-09-03"),
        session("archived", "2026-09-05", { archivedAt: "2026-09-05" }),
        session("other-project", "2026-09-06", { workspaceId: "other" }),
      ]}
    />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Resume conversation →" }),
  );
  await waitFor(() => expect(resume).toHaveBeenCalledWith("resume-this"));
  expect(conversation).not.toHaveBeenCalled();
});

it("keeps proposed research questions out of the brief while showing their pending decision", async () => {
  const data = home();
  data.notes = [
    {
      id: "proposed",
      workspaceId: "workspace",
      paperId: null,
      kind: "question",
      body: "An unaccepted claim",
      state: "proposed",
      origin: "assistant",
      pinned: true,
      revision: 1,
      createdAt: "2026-09-01",
      updatedAt: "2026-09-01",
    },
  ];
  data.settings.body.briefNoteIds = ["proposed"];
  mocks.home.mockResolvedValue(data);
  await mount();
  expect(
    screen.getByRole("region", { name: "Research brief" }),
  ).not.toHaveTextContent("An unaccepted claim");
  expect(
    screen.getByRole("button", { name: "Review suggested notes →" }),
  ).toBeVisible();
});

it("asks for the conversation first only when a project has nothing but conversations", async () => {
  const thread = {
    id: "thread",
    workspaceId: "workspace",
    title: "Idea",
    updatedAt: "2026-09-03",
    createdAt: "2026-09-03",
    paperId: null,
    presetId: null,
    overrides: {},
    draft: "",
    revision: 1,
    archivedAt: null,
  } satisfies WorkbenchSession;
  const first = vi.fn();
  const brief = vi.fn();
  const surface = (key: string) => (
    <WorkspaceProjectSurface
      key={key}
      workspaceId="workspace"
      onConversation={vi.fn()}
      onWorkspaceChanged={vi.fn()}
      onConversationFirst={first}
      onProjectBrief={brief}
      sessions={[thread]}
    />
  );
  // A folder is attached: the overview has state to show.
  const view = render(surface("with-folder"));
  await screen.findByRole("heading", { name: "Local research" });
  expect(first).not.toHaveBeenCalled();

  const data = home();
  data.inventory = null;
  data.notes = [
    {
      id: "question",
      workspaceId: "workspace",
      paperId: null,
      kind: "question",
      body: "Do minimum wages reduce teen employment?",
      state: "accepted",
      origin: "user",
      pinned: true,
      revision: 1,
      createdAt: "2026-09-01",
      updatedAt: "2026-09-01",
    },
  ];
  mocks.home.mockResolvedValue(data);
  mocks.getWorkspace.mockResolvedValue({
    id: "workspace",
    name: "Idea notebook",
    root: null,
    revision: 1,
  });
  view.rerender(surface("conversation-only"));
  await screen.findByRole("heading", { name: "Idea notebook" });
  await waitFor(() => expect(first).toHaveBeenCalledOnce());
  expect(brief).toHaveBeenLastCalledWith(
    "Do minimum wages reduce teen employment?",
  );
});
