import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Citations from "./Citations";
import GitHubCollaboration from "../version-control/GitHubCollaboration";
import ProjectChecks from "../project-surface/ProjectChecks";
import ProjectSnapshots from "../project-surface/ProjectSnapshots";
import type { ProjectHome } from "../../lib/projectClient";

const { invoke, save, open } = vi.hoisted(() => ({
  invoke: vi.fn(),
  save: vi.fn(),
  open: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open }));
beforeEach(() => {
  vi.clearAllMocks();
  invoke.mockResolvedValue([]);
});
afterEach(cleanup);

it("checks selected bibliography and draft text only on request, inserts a key, and marks a changed draft stale", async () => {
  const onInsert = vi.fn();
  invoke.mockResolvedValue({
    entries: [
      {
        key: "smith2024",
        title: "Growth",
        author: "Smith",
        path: "refs.bib",
        line: 2,
        warnings: [],
      },
    ],
    missing: ["unknown"],
    warnings: [],
    coverage: "Selected source only.",
    versions: { "refs.bib": "hash" },
    sourceHash: "draft",
  });
  const props = {
    workspaceId: "ws",
    paths: ["refs.bib"],
    sourcePath: "main.tex",
    sourceText: "\\cite{unknown}",
    checkpointId: null,
    disabled: false,
    onInsert,
  };
  const view = render(<Citations {...props} />);
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Citations"));
  fireEvent.click(screen.getByText("Index and check citations"));
  expect(await screen.findByText("Missing keys: unknown")).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith("workbench_citations", {
    workspaceId: "ws",
    request: {
      bibliographies: ["refs.bib"],
      sourcePath: "main.tex",
      sourceText: "\\cite{unknown}",
      checkpointId: null,
    },
  });
  fireEvent.click(screen.getByText("Insert citation"));
  expect(onInsert).toHaveBeenCalledWith("\\cite{smith2024}");
  view.rerender(<Citations {...props} sourceText="changed" />);
  expect(
    screen.getByText(/Text or selection changed; check again/),
  ).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledTimes(1);
});

it("reads GitHub only on request and prevents a prepared PR from publishing after the local head changes", async () => {
  invoke.mockImplementation(async (command: string) =>
    command === "workbench_github_inspect"
      ? {
          id: "snapshot",
          body: {
            repository: "owner/repo",
            branch: "research",
            head: "abc",
            checkedAt: "2026-11-23T12:00:00Z",
            scope: "Exact commit",
            pullRequests: [],
            runs: [],
            issues: [],
          },
        }
      : "https://github.com/owner/repo/pull/1",
  );
  const view = render(
    <GitHubCollaboration workspaceId="ws" head="abc" onRefresh={vi.fn()} />,
  );
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("GitHub collaboration"));
  fireEvent.click(screen.getByText("Check pull requests and CI"));
  fireEvent.click(await screen.findByText("Prepare draft pull request"));
  fireEvent.change(screen.getByLabelText("Pull request title"), {
    target: { value: "Research update" },
  });
  fireEvent.change(screen.getByLabelText("Pull request description"), {
    target: { value: "Selected result\n\nCheck receipt" },
  });
  await waitFor(() =>
    expect(screen.getByText("Create draft on GitHub")).toBeEnabled(),
  );
  view.rerender(
    <GitHubCollaboration workspaceId="ws" head="newer" onRefresh={vi.fn()} />,
  );
  expect(screen.getByText("Create draft on GitHub")).toBeDisabled();
  expect(invoke).toHaveBeenCalledTimes(1);
  view.rerender(
    <GitHubCollaboration workspaceId="ws" head="abc" onRefresh={vi.fn()} />,
  );
  fireEvent.click(screen.getByText("Create draft on GitHub"));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("workbench_github_draft", {
      workspaceId: "ws",
      request: {
        expectedHead: "abc",
        expectedRepository: "owner/repo",
        title: "Research update",
        base: "main",
        body: "Selected result\n\nCheck receipt",
      },
    }),
  );
});

it("loading check configuration and saving opt-in do not run analysis", async () => {
  invoke.mockImplementation(
    async (command: string, args: Record<string, unknown>) =>
      command === "workbench_check_policy"
        ? { revision: 1, body: args.policy }
        : [],
  );
  render(<ProjectChecks workspaceId="ws" />);
  expect(invoke).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Configure checks and view history"));
  const optIn = await screen.findByLabelText(
    "Check after saving one of these files in Pipeline",
  );
  expect(optIn).not.toBeChecked();
  fireEvent.click(optIn);
  fireEvent.click(screen.getByText("Save check settings"));
  await screen.findByText(
    "Check settings saved. Saving settings does not run a check.",
  );
  expect(invoke).not.toHaveBeenCalledWith(
    "workbench_check_run",
    expect.anything(),
  );
  expect(invoke).toHaveBeenCalledWith(
    "workbench_check_policy",
    expect.objectContaining({
      policy: {
        enabled: true,
        sourcePath: "main.tex",
        bibliographies: ["references.bib"],
      },
    }),
  );
});

it("exports the selected immutable snapshot identity without recapturing current files", async () => {
  const snapshot = {
    id: "submitted",
    title: "Journal submission",
    createdAt: "2026-11-23T12:00:00Z",
    contentHash: "immutable-hash",
    body: {
      handoff: "Reproduction instructions",
      files: [{ path: "files/main.tex", hash: "old-bytes", size: 20 }],
    },
  };
  invoke.mockImplementation(
    async (command: string, args: Record<string, unknown>) =>
      command === "workbench_desk_records" && args.kind === "submission"
        ? [snapshot]
        : [],
  );
  save.mockResolvedValue("C:/exports/submission.zip");
  render(
    <ProjectSnapshots
      workspaceId="ws"
      data={
        {
          notes: [],
          tasks: [],
          papers: [],
          executions: [],
          applications: [],
          inventory: null,
        } as unknown as ProjectHome
      }
    />,
  );
  fireEvent.click(screen.getByText("Load snapshots and evidence"));
  fireEvent.click(await screen.findByText(/Journal submission ·/));
  fireEvent.click(screen.getByText("Export this snapshot"));
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("workbench_submission_export", {
      workspaceId: "ws",
      id: "submitted",
      expectedHash: "immutable-hash",
      path: "C:/exports/submission.zip",
    }),
  );
  expect(invoke).not.toHaveBeenCalledWith(
    "workbench_submission_capture",
    expect.anything(),
  );
});
