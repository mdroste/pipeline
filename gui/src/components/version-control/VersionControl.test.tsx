import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import VersionControl from "./VersionControl";
import { confirmDialog } from "../DialogService";
import type {
  ChangedFile,
  VersionControlState,
} from "../../lib/versionControlClient";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("../DialogService", () => ({
  confirmDialog: vi.fn(),
  notify: vi.fn(),
  promptDialog: vi.fn(),
}));

const file = (
  path: string,
  overrides: Partial<ChangedFile> = {},
): ChangedFile => ({
  path,
  oldPath: null,
  status: "modified",
  staged: false,
  unstaged: true,
  isDirectory: false,
  isSubmodule: false,
  conflict: null,
  ...overrides,
});

const repository = (
  overrides: Partial<Extract<VersionControlState, { state: "ready" }>> = {},
  status: object = {},
): VersionControlState => ({
  state: "ready",
  status: {
    branch: "main",
    head: "a".repeat(40),
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    fetchedAt: new Date().toISOString(),
    changed: 0,
    untracked: 0,
    conflicts: 0,
    changedPaths: [],
    remote: {
      name: "origin",
      host: "github.com",
      owner: "mdroste",
      repo: "minwage",
      provider: "github",
      webUrl: "https://github.com/mdroste/minwage",
    },
    commits: [],
    incoming: [],
    ...status,
  },
  files: [],
  filesTruncated: false,
  largeFiles: [],
  branches: [
    { name: "main", current: true, upstream: "origin/main", remoteRef: null },
    { name: "draft", current: false, upstream: null, remoteRef: null },
  ],
  operation: null,
  operationMessage: null,
  outsideConflicts: 0,
  identity: { name: "Ada", email: "ada@example.org" },
  folderPrefix: "",
  canUndoCommit: false,
  ...overrides,
});

const mock = vi.mocked(invoke);
const stub = (
  state: VersionControlState,
  after: VersionControlState = state,
) => {
  mock.mockImplementation(async (command: string) => {
    if (command === "workbench_version_control_state") return state;
    if (command === "workbench_version_control_action")
      return { state: after, notice: null, undoneMessage: null };
    if (command === "workbench_version_control_diff")
      return { kind: "diff", text: "@@ -1 +1 @@\n-old line\n+new line\n" };
    if (command === "workbench_version_control_history")
      return { commits: [], hasMore: false };
    return undefined;
  });
};
const actions = () =>
  mock.mock.calls
    .filter(([command]) => command === "workbench_version_control_action")
    .map(([, args]) => (args as { action: unknown }).action);

// The page keeps its state by project, so each test gets a project of its own.
let projects = 0;
let project = "";
const page = (workspaceId = project) =>
  render(
    <VersionControl
      workspaceId={workspaceId}
      active
      onAttachFolder={vi.fn()}
      onFilesChanged={vi.fn()}
    />,
  );

beforeEach(() => {
  project = `ws-${++projects}`;
  localStorage.clear();
  mock.mockReset();
  vi.mocked(openExternal).mockReset();
  vi.mocked(confirmDialog).mockReset().mockResolvedValue(false);
});

it("leads a folder with no repository to one first step", async () => {
  stub({ state: "notRepository", problem: null }, repository());
  page();
  fireEvent.click(
    await screen.findByRole("button", { name: "Start tracking this folder" }),
  );
  await waitFor(() => expect(actions()).toEqual([{ kind: "initialize" }]));
  expect(
    await screen.findByRole("button", { name: /Branch\s*main/ }),
  ).toBeVisible();
});

it("explains a missing Git and a folder Git refuses", async () => {
  stub({ state: "gitMissing" });
  const first = page();
  expect(
    await screen.findByText("Git isn't installed on this computer."),
  ).toBeVisible();
  first.unmount();
  stub({ state: "notRepository", problem: "Git refuses this folder." });
  page();
  expect(await screen.findByText("Git refuses this folder.")).toBeVisible();
  expect(screen.queryByRole("button", { name: /Start tracking/ })).toBeNull();
});

it("commits the checked files with the summary and clears the box", async () => {
  stub(
    repository({
      files: [file("paper.tex"), file("notes.txt", { status: "untracked" })],
    }),
    repository(),
  );
  page("commits-checked-files");
  expect(await screen.findByText("new line")).toBeVisible();
  fireEvent.click(screen.getByRole("checkbox", { name: "Include notes.txt" }));
  const commit = screen.getByRole("button", { name: "Commit 1 file to main" });
  expect(commit).toBeDisabled();
  fireEvent.change(screen.getByRole("textbox", { name: "Commit summary" }), {
    target: { value: "Tighten the introduction" },
  });
  fireEvent.click(commit);
  await waitFor(() =>
    expect(actions()).toEqual([
      {
        kind: "commit",
        paths: ["paper.tex"],
        summary: "Tighten the introduction",
        description: null,
      },
    ]),
  );
  expect(await screen.findByText(/Nothing to commit/)).toBeVisible();
});

/** Writes a summary and commits whatever is checked. */
async function commitChecked(path = "paper.tex") {
  fireEvent.change(screen.getByRole("textbox", { name: "Commit summary" }), {
    target: { value: "Tighten the introduction" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Commit 1 file to main" }),
  );
  await waitFor(() =>
    expect(actions()).toEqual([
      expect.objectContaining({ kind: "commit", paths: [path] }),
    ]),
  );
}
const leftOut = () =>
  repository({ files: [file("paper.tex"), file("secrets.csv")] });

it("keeps a file left out after a look at History", async () => {
  stub(leftOut());
  page("left-out-history");
  expect(await screen.findByText("new line")).toBeVisible();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "Include secrets.csv" }),
  );
  fireEvent.click(screen.getByRole("radio", { name: "History" }));
  fireEvent.click(await screen.findByRole("radio", { name: /^Changes/ }));
  expect(
    await screen.findByRole("checkbox", { name: "Include secrets.csv" }),
  ).not.toBeChecked();
  await commitChecked();
});

it("keeps a file left out after Open in Files closes the page", async () => {
  stub(leftOut());
  const first = page("left-out-files");
  expect(await screen.findByText("new line")).toBeVisible();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "Include secrets.csv" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Open in Files" }));
  // Changing project tabs unmounts this page.
  first.unmount();
  page("left-out-files");
  expect(
    await screen.findByRole("checkbox", { name: "Include secrets.csv" }),
  ).not.toBeChecked();
  await commitChecked();
});

it("keeps each project's unchecked files to itself", async () => {
  stub(leftOut());
  const first = page("left-out-one");
  fireEvent.click(
    await screen.findByRole("checkbox", { name: "Include secrets.csv" }),
  );
  first.unmount();
  page("left-out-other");
  expect(
    await screen.findByRole("checkbox", { name: "Include secrets.csv" }),
  ).toBeChecked();
});

it("asks for a name and email before the first commit", async () => {
  stub(repository({ files: [file("paper.tex")], identity: null }));
  page();
  fireEvent.change(await screen.findByRole("textbox", { name: "Your name" }), {
    target: { value: "Ada" },
  });
  fireEvent.change(screen.getByRole("textbox", { name: "Your email" }), {
    target: { value: "ada@example.org" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(actions()).toEqual([
      { kind: "setIdentity", name: "Ada", email: "ada@example.org" },
    ]),
  );
});

it("confirms before discarding a file", async () => {
  stub(repository({ files: [file("paper.tex")] }));
  page();
  fireEvent.click(
    await screen.findByRole("button", { name: "More actions for paper.tex" }),
  );
  fireEvent.click(screen.getByRole("menuitem", { name: "Discard changes" }));
  await waitFor(() => expect(confirmDialog).toHaveBeenCalledOnce());
  expect(vi.mocked(confirmDialog).mock.calls[0][0]).toContain(
    "goes to the Trash",
  );
  expect(actions()).toEqual([]);

  vi.mocked(confirmDialog).mockResolvedValue(true);
  fireEvent.click(
    screen.getByRole("button", { name: "More actions for paper.tex" }),
  );
  fireEvent.click(screen.getByRole("menuitem", { name: "Discard changes" }));
  await waitFor(() =>
    expect(actions()).toEqual([{ kind: "discard", paths: ["paper.tex"] }]),
  );
});

it("offers the next sync step for the branch's state", async () => {
  stub(repository({}, { behind: 2, ahead: 1 }));
  const behind = page();
  fireEvent.click(await screen.findByRole("button", { name: "Pull 2" }));
  await waitFor(() => expect(actions()).toEqual([{ kind: "pull" }]));
  behind.unmount();

  stub(repository({}, { ahead: 3 }));
  const ahead = page();
  expect(await screen.findByRole("button", { name: "Push 3" })).toBeVisible();
  ahead.unmount();

  stub(repository({}, { upstream: null }));
  const local = page();
  expect(
    await screen.findByRole("button", { name: "Publish branch" }),
  ).toBeVisible();
  local.unmount();

  stub(repository({}, { remote: null, upstream: null }));
  page();
  fireEvent.click(
    await screen.findByRole("button", { name: "Connect remote" }),
  );
  expect(await screen.findByText(/paste its address/)).toBeVisible();
});

it("offers sync for a remote whose address shows no host", async () => {
  const remote = {
    name: "origin",
    host: "",
    owner: "",
    repo: "",
    provider: null,
    webUrl: null,
  };
  stub(repository({}, { ahead: 1, remote }));
  page();
  expect(await screen.findByRole("button", { name: "Push 1" })).toBeVisible();
  expect(screen.getByText("Remote origin")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Connect remote" })).toBeNull();
  fireEvent.click(
    screen.getByRole("button", { name: "More version control actions" }),
  );
  expect(
    screen.getByRole("menuitem", { name: "Check the remote for updates" }),
  ).toBeVisible();
  expect(
    screen.getByRole("menuitem", { name: "Change remote…" }),
  ).toBeVisible();
});

it("keeps the commit message after the page closes", async () => {
  stub(repository({ files: [file("paper.tex")] }));
  const first = page();
  fireEvent.change(
    await screen.findByRole("textbox", { name: "Commit summary" }),
    { target: { value: "Tighten the introduction" } },
  );
  fireEvent.change(
    screen.getByRole("textbox", { name: "Commit description" }),
    {
      target: { value: "A long description of the change." },
    },
  );
  fireEvent.click(screen.getByRole("button", { name: "Open in Files" }));
  first.unmount();
  page();
  expect(
    await screen.findByRole("textbox", { name: "Commit summary" }),
  ).toHaveValue("Tighten the introduction");
  expect(
    screen.getByRole("textbox", { name: "Commit description" }),
  ).toHaveValue("A long description of the change.");
});

it("keeps each project's commit message to itself", async () => {
  stub(repository({ files: [file("paper.tex")] }));
  const first = page();
  fireEvent.change(
    await screen.findByRole("textbox", { name: "Commit summary" }),
    { target: { value: "Only this project" } },
  );
  first.unmount();
  page(`${project}-other`);
  expect(
    await screen.findByRole("textbox", { name: "Commit summary" }),
  ).toHaveValue("");
});

/** Stubs Git with a push that waits, and returns how to finish it. */
function pendingPush(after: VersionControlState) {
  const pending: {
    finish: (result: unknown) => void;
    fail: (error: unknown) => void;
  } = { finish: () => undefined, fail: () => undefined };
  const state = repository({}, { ahead: 2 });
  mock.mockImplementation(async (command: string) => {
    if (command === "workbench_version_control_state") return state;
    if (command === "workbench_version_control_action")
      return new Promise((resolve, reject) => {
        pending.finish = resolve;
        pending.fail = reject;
      });
    return undefined;
  });
  return {
    pending,
    done: { state: after, notice: null, undoneMessage: null },
  };
}

it("brings back Stop when the page returns to a running push", async () => {
  const { pending, done } = pendingPush(repository());
  const first = page();
  fireEvent.click(await screen.findByRole("button", { name: "Push 2" }));
  expect(await screen.findByRole("button", { name: "Stop" })).toBeVisible();
  first.unmount();

  page();
  fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
  await waitFor(() =>
    expect(mock).toHaveBeenCalledWith("workbench_version_control_cancel", {
      workspaceId: project,
    }),
  );
  expect(screen.getByText("Pushing…")).toBeVisible();
  pending.finish(done);
  expect(
    await screen.findByRole("button", { name: "Check for updates" }),
  ).toBeVisible();
  expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
});

it("shows a push's failure on the way back to the page", async () => {
  const { pending } = pendingPush(repository());
  const first = page();
  fireEvent.click(await screen.findByRole("button", { name: "Push 2" }));
  await screen.findByRole("button", { name: "Stop" });
  first.unmount();
  pending.fail({ message: "The remote refused this push." });
  await waitFor(() => expect(screen.queryByText("Pushing…")).toBeNull());

  page();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The remote refused this push.",
  );
  expect(await screen.findByRole("button", { name: "Push 2" })).toBeEnabled();
});

it("asks before Git passes over an unsaved draft", async () => {
  localStorage.setItem(
    `pipeline.files.draft.workspace:${project}:working.paper.tex`,
    "{}",
  );
  stub(repository({ files: [file("paper.tex"), file("notes.txt")] }));
  page();
  fireEvent.click(await screen.findByRole("button", { name: /Branch\s*main/ }));
  fireEvent.click(screen.getByRole("menuitem", { name: "draft" }));
  await waitFor(() => expect(confirmDialog).toHaveBeenCalledOnce());
  expect(vi.mocked(confirmDialog).mock.calls[0][0]).toContain(
    "unsaved draft of paper.tex",
  );
  await vi.mocked(confirmDialog).mock.results[0].value;
  expect(actions()).toEqual([]);

  // A commit that leaves the file out has nothing to warn about.
  vi.mocked(confirmDialog).mockClear();
  fireEvent.click(screen.getByRole("checkbox", { name: "Include paper.tex" }));
  await commitChecked("notes.txt");
  expect(confirmDialog).not.toHaveBeenCalled();
});

it("shows Git's refusal without losing the commit message", async () => {
  stub(repository({ files: [file("paper.tex")] }));
  page();
  fireEvent.change(
    await screen.findByRole("textbox", { name: "Commit summary" }),
    {
      target: { value: "Keep me" },
    },
  );
  mock.mockImplementation(async (command: string) => {
    if (command === "workbench_version_control_action")
      throw { message: "tests must pass" };
    if (command === "workbench_version_control_diff")
      return { kind: "empty", text: "" };
    return repository({ files: [file("paper.tex")] });
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Commit 1 file to main" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent("tests must pass");
  expect(screen.getByRole("textbox", { name: "Commit summary" })).toHaveValue(
    "Keep me",
  );
});

it("walks a conflict through to a merge commit", async () => {
  const conflicted = file("paper.tex", {
    status: "conflicted",
    conflict: "changed on both sides",
    unstaged: false,
  });
  const merging = repository({
    files: [
      conflicted,
      file("incoming.txt", { status: "added", staged: true }),
    ],
    operation: "merge",
    operationMessage: "Merge branch 'main' of github.com:mdroste/minwage",
  });
  stub(merging);
  vi.mocked(confirmDialog).mockResolvedValue(true);
  page();
  expect(await screen.findByText(/1 file needs a decision/)).toBeVisible();
  expect(screen.queryByRole("checkbox")).toBeNull();
  expect(
    screen.getByRole("button", { name: "Resolve 1 conflict first" }),
  ).toBeDisabled();
  await waitFor(() =>
    expect(screen.getByRole("textbox", { name: "Commit summary" })).toHaveValue(
      "Merge branch 'main' of github.com:mdroste/minwage",
    ),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Keep incoming version" }),
  );
  await waitFor(() =>
    expect(actions()).toEqual([
      { kind: "resolve", path: "paper.tex", choice: "theirs" },
    ]),
  );
});

it("confirms choosing a side, and a hand fix needs no confirmation", async () => {
  stub(
    repository({
      files: [
        file("paper.tex", {
          status: "conflicted",
          conflict: "changed on both sides",
          unstaged: false,
        }),
      ],
      operation: "merge",
    }),
  );
  page();
  fireEvent.click(
    await screen.findByRole("button", { name: /Keep this branch/ }),
  );
  await waitFor(() => expect(confirmDialog).toHaveBeenCalledOnce());
  expect(vi.mocked(confirmDialog).mock.calls[0][0]).toContain(
    "goes to the Trash",
  );
  await vi.mocked(confirmDialog).mock.results[0].value;
  expect(actions()).toEqual([]);

  vi.mocked(confirmDialog).mockResolvedValue(true);
  fireEvent.click(
    screen.getByRole("button", { name: "Keep incoming version" }),
  );
  await waitFor(() =>
    expect(actions()).toEqual([
      { kind: "resolve", path: "paper.tex", choice: "theirs" },
    ]),
  );

  vi.mocked(confirmDialog).mockClear();
  fireEvent.click(
    await screen.findByRole("button", { name: "I fixed this file" }),
  );
  await waitFor(() => expect(actions()).toHaveLength(2));
  expect(confirmDialog).not.toHaveBeenCalled();
});

it("keeps the commit box when keeping one side leaves no changes", async () => {
  stub(
    repository({ operation: "merge", operationMessage: "Merge origin/main" }),
  );
  page();
  const commit = await screen.findByRole("button", { name: "Commit merge" });
  await waitFor(() => expect(commit).toBeEnabled());
  expect(screen.getByText(/All conflicts are resolved/)).toBeVisible();
});

it("blocks a merge commit while a conflict sits outside the project", async () => {
  stub(repository({ operation: "merge", outsideConflicts: 2 }));
  page();
  expect(
    await screen.findByText(/2 conflicted files are outside this project/),
  ).toBeVisible();
  expect(
    screen.getByRole("button", { name: "Resolve 2 conflicts first" }),
  ).toBeDisabled();
});

it("takes back Git's merge message if the merge is aborted", async () => {
  vi.mocked(confirmDialog).mockResolvedValue(true);
  stub(
    repository({
      files: [file("paper.tex")],
      operation: "merge",
      operationMessage: "Merge origin/main",
    }),
    repository({ files: [file("paper.tex")] }),
  );
  page();
  const box = await screen.findByRole("textbox", { name: "Commit summary" });
  await waitFor(() => expect(box).toHaveValue("Merge origin/main"));
  fireEvent.click(screen.getByRole("button", { name: "Abort" }));
  await waitFor(() =>
    expect(screen.getByRole("textbox", { name: "Commit summary" })).toHaveValue(
      "",
    ),
  );
});

it("keeps a failed action's message through background refreshes", async () => {
  stub(repository({ files: [file("paper.tex")] }));
  page();
  fireEvent.change(
    await screen.findByRole("textbox", { name: "Commit summary" }),
    { target: { value: "Blocked" } },
  );
  mock.mockImplementation(async (command: string) => {
    if (command === "workbench_version_control_action")
      throw { message: "tests must pass" };
    if (command === "workbench_version_control_diff")
      return { kind: "empty", text: "" };
    return repository({ files: [file("paper.tex")] });
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Commit 1 file to main" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent("tests must pass");
  fireEvent.focus(window);
  await waitFor(() =>
    expect(
      mock.mock.calls.filter(
        ([command]) => command === "workbench_version_control_state",
      ).length,
    ).toBeGreaterThan(1),
  );
  expect(screen.getByRole("alert")).toHaveTextContent("tests must pass");
});

it("warns before leaving a detached commit", async () => {
  stub(repository({}, { branch: null }));
  page();
  fireEvent.click(
    await screen.findByRole("button", { name: /Branch\s*No branch/ }),
  );
  fireEvent.click(screen.getByRole("menuitem", { name: "draft" }));
  await waitFor(() => expect(confirmDialog).toHaveBeenCalledOnce());
  expect(actions()).toEqual([]);
});

it("opens a pull request from a pushed feature branch on each host", async () => {
  stub(repository({}, { branch: "referee/round-2" }));
  const github = page();
  fireEvent.click(
    await screen.findByRole("button", { name: "Open pull request" }),
  );
  expect(openExternal).toHaveBeenCalledWith(
    "https://github.com/mdroste/minwage/compare/referee/round-2?expand=1",
  );
  github.unmount();
  stub(
    repository(
      {},
      {
        branch: "referee/round-2",
        remote: {
          name: "origin",
          host: "bitbucket.org",
          owner: "lab",
          repo: "minwage",
          provider: "bitbucket",
          webUrl: "https://bitbucket.org/lab/minwage",
        },
      },
    ),
  );
  page();
  fireEvent.click(
    await screen.findByRole("button", { name: "Open pull request" }),
  );
  expect(openExternal).toHaveBeenLastCalledWith(
    "https://bitbucket.org/lab/minwage/pull-requests/new?source=referee%2Fround-2&t=1",
  );
  expect(screen.getByText(/lab\/minwage on Bitbucket/)).toBeVisible();
});

it("shows history with what has not been pushed", async () => {
  stub(repository());
  mock.mockImplementation(async (command: string) => {
    if (command === "workbench_version_control_history")
      return {
        commits: [
          {
            sha: "b".repeat(40),
            author: "Ada",
            date: "2026-10-01T10:00:00Z",
            subject: "Re-estimate Table 4",
            isMerge: false,
            refs: ["v1.0", "origin/main"],
            unpushed: true,
          },
        ],
        hasMore: false,
      };
    if (command === "workbench_version_control_commit")
      return {
        sha: "b".repeat(40),
        author: "Ada",
        date: "2026-10-01T10:00:00Z",
        message: "Re-estimate Table 4\n\nCluster by commuting zone.",
        files: [
          { path: "output/table4.tex", oldPath: null, status: "modified" },
        ],
        filesTruncated: false,
      };
    if (command === "workbench_version_control_diff")
      return { kind: "binary", text: "" };
    return repository();
  });
  page();
  fireEvent.click(await screen.findByRole("radio", { name: "History" }));
  expect(await screen.findByText("Not pushed")).toBeVisible();
  expect(screen.getByText("v1.0")).toBeVisible();
  expect(await screen.findByText("Cluster by commuting zone.")).toBeVisible();
  expect(await screen.findByText(/not a text file/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Open on GitHub" }));
  expect(openExternal).toHaveBeenCalledWith(
    `https://github.com/mdroste/minwage/commit/${"b".repeat(40)}`,
  );
});
