import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ProjectsPage from "./ProjectsPage";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const project = {
  schema_version: 1,
  id: "paper",
  name: "Paper revision",
  description: "R&R",
  created: "2026-08-01T00:00:00Z",
  updated: "2026-08-02T00:00:00Z",
  run_ids: [],
};
const run = {
  run_id: "run_1",
  created: "2026-08-02T00:00:00Z",
  input_name: "paper.pdf",
  input_path: "/papers/paper.pdf",
  input_mode: "document",
  input_interpretation: "document",
  profile_id: "review",
  profile_name: "Review",
  provider: "codex",
  status: "done",
  duration_secs: 10,
  input_tokens: 10,
  output_tokens: 2,
  cached_input_tokens: 0,
  cache_write_input_tokens: 0,
  model_round_trips: 1,
  tool_calls: { text_file: 0, image: 0, web: 0, shell_or_other: 0, unknown: 0 },
  step_count: 1,
  artifact_count: 2,
  failed_steps: [],
  resumable: true,
  title: "Revision 2",
  tags: [],
};

describe("ProjectsPage", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation((command: string, args?: { included?: boolean }) => {
      if (command === "list_projects") return Promise.resolve({ projects: [project], warnings: [] });
      if (command === "list_runs") return Promise.resolve([run]);
      if (command === "sync_project_issue_ledger") {
        return Promise.resolve({
          schema_version: 1,
          project_id: project.id,
          updated: project.updated,
          issues: [],
          warnings: [],
        });
      }
      if (command === "set_project_run") {
        return Promise.resolve({ ...project, run_ids: args?.included ? [run.run_id] : [] });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
  });

  it("adds an existing run and opens it from the project", async () => {
    const user = userEvent.setup();
    const onOpenRun = vi.fn();
    render(<ProjectsPage onOpenRun={onOpenRun} />);

    expect(await screen.findByText("Paper revision")).toBeVisible();
    await user.selectOptions(screen.getByRole("combobox", { name: "Report to add" }), run.run_id);
    await user.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_project_run", {
      projectId: "paper",
      runId: "run_1",
      included: true,
    }));
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(onOpenRun).toHaveBeenCalledWith("run_1");
  });
});
