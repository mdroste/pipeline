import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ProjectReviews from "./ProjectReviews";
import { router } from "../../lib/router";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

const run = (id: string, path: string) => ({
  run_id: id,
  created: "2026-09-28T21:14:00Z",
  input_name: path.split("/").pop(),
  input_path: path,
  input_mode: "document",
  profile_id: "auto",
  profile_name: "Auto Paper Review",
  provider: "claude",
  status: "complete",
  duration_secs: 100,
  input_tokens: 1,
  output_tokens: 1,
  cached_input_tokens: 0,
  cache_write_input_tokens: 0,
  step_count: 5,
  artifact_count: 2,
  failed_steps: [],
  resumable: false,
  title: `Review of ${path.split("/").pop()}`,
  tags: [],
});

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === "list_runs")
      return [
        run("in-project", "/papers/minwage/draft_v7.pdf"),
        run("elsewhere", "/papers/other/thing.pdf"),
      ];
    if (command === "list_projects")
      return {
        projects: [
          {
            schema_version: 1,
            id: "col-1",
            name: "Minimum wage revision",
            description: "",
            created: "2026-08-30",
            updated: "2026-09-29",
            run_ids: ["in-project"],
          },
          {
            schema_version: 1,
            id: "col-2",
            name: "Unrelated collection",
            description: "",
            created: "2026-08-30",
            updated: "2026-09-29",
            run_ids: ["elsewhere"],
          },
        ],
        warnings: [],
      };
    if (command === "sync_project_issue_ledger")
      return {
        schema_version: 1,
        project_id: "col-1",
        updated: "2026-09-29",
        issues: [],
        warnings: [],
      };
    throw new Error(`unexpected ${command}`);
  });
});

it("scopes runs and ledgers to the project folder and files the rest away", async () => {
  render(<ProjectReviews workspaceRoot="/papers/minwage" />);
  // The run under the project root is listed; the other is not.
  expect(await screen.findByText("Review of draft_v7.pdf")).toBeInTheDocument();
  expect(screen.queryByText("Review of thing.pdf")).not.toBeInTheDocument();
  // The linked collection's ledger renders in place.
  expect(
    await screen.findByLabelText("Minimum wage revision findings"),
  ).toBeInTheDocument();
  // Collections with no runs in this project stay out of the way.
  expect(
    screen.getByRole("button", { name: "Unrelated collection" }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Start a review" }),
  ).toBeInTheDocument();
});

const collection = (id: string, name: string, run_ids: string[]) => ({
  schema_version: 1,
  id,
  name,
  description: "",
  created: "2026-08-30",
  updated: "2026-09-29",
  run_ids,
});

function reviewsWith(collections: ReturnType<typeof collection>[]) {
  const stored = new Map(collections.map((item) => [item.id, item]));
  mocks.invoke.mockImplementation(
    async (command: string, args?: Record<string, string>) => {
      if (command === "list_runs")
        return [run("in-project", "/papers/minwage/draft_v7.pdf")];
      if (command === "list_projects")
        return { projects: [...stored.values()], warnings: [] };
      if (command === "create_project") {
        const created = collection("new", args!.name, []);
        stored.set(created.id, created);
        return created;
      }
      if (command === "set_project_run") {
        const item = stored.get(args!.projectId)!;
        const updated = {
          ...item,
          run_ids: [...item.run_ids, args!.runId],
        };
        stored.set(updated.id, updated);
        return updated;
      }
      if (command === "sync_project_issue_ledger")
        return {
          schema_version: 1,
          project_id: args!.projectId,
          updated: "2026-09-29",
          issues: [],
          warnings: [],
        };
      throw new Error(`unexpected ${command}`);
    },
  );
}

it("offers to track findings when no collection holds the project's reviews", async () => {
  reviewsWith([]);
  const onChanged = vi.fn();
  render(
    <ProjectReviews
      workspaceRoot="/papers/minwage"
      workspaceName="Min wage"
      onChanged={onChanged}
    />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Track findings" }),
  );
  expect(
    await screen.findByLabelText("Min wage findings findings"),
  ).toBeInTheDocument();
  expect(mocks.invoke).toHaveBeenCalledWith("create_project", {
    name: "Min wage findings",
    description: null,
  });
  expect(mocks.invoke).toHaveBeenCalledWith("set_project_run", {
    projectId: "new",
    runId: "in-project",
    included: true,
  });
  expect(screen.queryByRole("button", { name: "Track findings" })).toBeNull();
  expect(onChanged).toHaveBeenCalledOnce();
});

it("adds a later review to the collection already tracking the project", async () => {
  reviewsWith([collection("old", "Revision", ["earlier"])]);
  const list = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation(async (command: string, args?: never) =>
    command === "list_runs"
      ? [
          run("earlier", "/papers/minwage/draft_v6.pdf"),
          run("in-project", "/papers/minwage/draft_v7.pdf"),
        ]
      : list(command, args),
  );
  render(<ProjectReviews workspaceRoot="/papers/minwage" />);
  expect(
    await screen.findByText("1 review isn’t in Findings across reviews yet."),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Track findings" }));
  await waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith("set_project_run", {
      projectId: "old",
      runId: "in-project",
      included: true,
    }),
  );
  expect(mocks.invoke).not.toHaveBeenCalledWith(
    "create_project",
    expect.anything(),
  );
});

it("shows why tracking failed", async () => {
  reviewsWith([]);
  const list = mocks.invoke.getMockImplementation()!;
  mocks.invoke.mockImplementation(async (command: string, args?: never) => {
    if (command === "create_project")
      throw { code: "conflict", message: "Collections are locked" };
    return list(command, args);
  });
  render(<ProjectReviews workspaceRoot="/papers/minwage" />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Track findings" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Collections are locked",
  );
});

it("asks for a folder when the project has none", async () => {
  render(<ProjectReviews workspaceRoot={null} />);
  expect(
    await screen.findByText(/Attach a folder to this project/),
  ).toBeInTheDocument();
  expect(
    screen.getByText("No reviews of this project's documents yet."),
  ).toBeInTheDocument();
});

it("opens the collection a button names", async () => {
  render(<ProjectReviews workspaceRoot="/papers/minwage" />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Unrelated collection" }),
  );
  await waitFor(() =>
    expect(router.route).toEqual({ page: "projects", collectionId: "col-2" }),
  );
});
