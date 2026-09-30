import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ProjectReviews from "./ProjectReviews";

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

it("asks for a folder when the project has none", async () => {
  render(<ProjectReviews workspaceRoot={null} />);
  expect(
    await screen.findByText(/Attach a folder to this project/),
  ).toBeInTheDocument();
  expect(
    screen.getByText("No reviews of this project's documents yet."),
  ).toBeInTheDocument();
});
