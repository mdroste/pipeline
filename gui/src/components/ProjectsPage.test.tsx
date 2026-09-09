import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
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
    invoke.mockImplementation(
      (command: string, args?: { included?: boolean }) => {
        if (command === "list_projects")
          return Promise.resolve({ projects: [project], warnings: [] });
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
          return Promise.resolve({
            ...project,
            run_ids: args?.included ? [run.run_id] : [],
          });
        }
        return Promise.reject(new Error(`unexpected command: ${command}`));
      },
    );
  });

  it("adds an existing run and opens it from the project", async () => {
    const user = userEvent.setup();
    const onOpenRun = vi.fn();
    render(<ProjectsPage onOpenRun={onOpenRun} />);

    expect(await screen.findByText("Paper revision")).toBeVisible();
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Report to add" }),
      run.run_id,
    );
    await user.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_project_run", {
        projectId: "paper",
        runId: "run_1",
        included: true,
      }),
    );
    await user.click(await screen.findByRole("button", { name: "Open" }));
    expect(onOpenRun).toHaveBeenCalledWith("run_1");
  });
  function twoProjects(
    update: (args: {
      id: string;
      name: string;
      description: string;
    }) => Promise<unknown>,
  ) {
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command, args) => {
      if (command === "list_projects")
        return Promise.resolve({
          projects: [
            project,
            { ...project, id: "other", name: "Other collection" },
          ],
          warnings: [],
        });
      if (command === "update_project") return update(args);
      return original(command, args);
    });
  }

  it("keeps a failed edit selected and retries before switching collections", async () => {
    const update = vi
      .fn()
      .mockRejectedValueOnce(new Error("Disk full"))
      .mockImplementation(async (args) => ({ ...project, ...args }));
    twoProjects(update);
    render(<ProjectsPage onOpenRun={vi.fn()} />);
    await screen.findByDisplayValue("Paper revision");
    fireEvent.change(screen.getByLabelText("Collection name"), {
      target: { value: "Revised title" },
    });
    fireEvent.change(screen.getByLabelText("Collection description"), {
      target: { value: "Keep this description" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Other collection/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Your changes have been kept",
    );
    expect(screen.getByLabelText("Collection name")).toHaveValue(
      "Revised title",
    );
    expect(screen.getByLabelText("Collection description")).toHaveValue(
      "Keep this description",
    );
    expect(
      screen.getByRole("button", { name: /Paper revision.*run/ }),
    ).toHaveAttribute("aria-current", "page");
    fireEvent.click(screen.getByRole("button", { name: /Other collection/ }));
    await screen.findByDisplayValue("Other collection");
    expect(update).toHaveBeenLastCalledWith({
      id: "paper",
      name: "Revised title",
      description: "Keep this description",
    });
  });

  it("serializes edits made during a write and drains them before switching", async () => {
    const writes: Array<{
      args: { id: string; name: string; description: string };
      resolve: (value: unknown) => void;
    }> = [];
    twoProjects(
      (args) => new Promise((resolve) => writes.push({ args, resolve })),
    );
    render(<ProjectsPage onOpenRun={vi.fn()} />);
    await screen.findByDisplayValue("Paper revision");
    fireEvent.change(screen.getByLabelText("Collection name"), {
      target: { value: "First edit" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save details" }));
    fireEvent.change(screen.getByLabelText("Collection name"), {
      target: { value: "Latest edit" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Other collection/ }));
    expect(writes).toHaveLength(1);
    await act(async () => writes[0].resolve({ ...project, ...writes[0].args }));
    expect(writes).toHaveLength(2);
    expect(screen.getByLabelText("Collection name")).toHaveValue("Latest edit");
    expect(writes[1].args.name).toBe("Latest edit");
    await act(async () => writes[1].resolve({ ...project, ...writes[1].args }));
    await screen.findByDisplayValue("Other collection");
  });

  it("registers a save guard and rejects leaving with an invalid or failed draft", async () => {
    twoProjects(async () => {
      throw new Error("Read-only folder");
    });
    const register = vi.fn();
    const dirty = vi.fn();
    const { unmount } = render(
      <ProjectsPage
        onOpenRun={vi.fn()}
        onDirtyChange={dirty}
        onSaveHandlerChange={register}
      />,
    );
    await screen.findByDisplayValue("Paper revision");
    const save = register.mock.calls[
      register.mock.calls.length - 1
    ][0] as () => Promise<boolean>;
    fireEvent.change(screen.getByLabelText("Collection name"), {
      target: { value: "" },
    });
    await act(async () => {
      expect(await save()).toBe(false);
    });
    expect(dirty).toHaveBeenLastCalledWith(true);
    fireEvent.change(screen.getByLabelText("Collection name"), {
      target: { value: "Keep this draft" },
    });
    await act(async () => {
      expect(await save()).toBe(false);
    });
    expect(screen.getByLabelText("Collection name")).toHaveValue(
      "Keep this draft",
    );
    unmount();
    expect(register).toHaveBeenLastCalledWith(null);
    expect(dirty).toHaveBeenLastCalledWith(false);
  });
});
