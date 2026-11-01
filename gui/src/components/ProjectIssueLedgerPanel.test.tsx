import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ProjectIssueLedgerPanel from "./ProjectIssueLedgerPanel";

const invoke = vi.hoisted(() => vi.fn());
const confirm = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./DialogService", async (original) => ({
  ...(await original<typeof import("./DialogService")>()),
  confirmDialog: confirm,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const project = {
  schema_version: 1,
  id: "mixed-project",
  name: "Mixed review project",
  description: "",
  created: "2026-08-01T00:00:00Z",
  updated: "2026-08-10T00:00:00Z",
  run_ids: ["run_paper", "run_code"],
};

const runs = [
  { run_id: "run_paper", input_name: "paper.pdf", title: "Paper review" },
  { run_id: "run_code", input_name: "src", title: "Code review" },
] as never[];

const ledger = {
  schema_version: 1,
  project_id: project.id,
  updated: "2026-08-10T00:00:00Z",
  warnings: [],
  issues: [
    {
      id: "issue-paper",
      title: "Identification assumption is unstated",
      severity: "high",
      section: "Model",
      status: "open",
      note: "",
      created: "2026-08-02T00:00:00Z",
      updated: "2026-08-02T00:00:00Z",
      decision_updated: "",
      occurrences: [
        {
          key: "occ-paper",
          run_id: "run_paper",
          issue_id: "ID-1",
          observed_at: "2026-08-02T00:00:00Z",
          profile_id: "paper-review",
          profile_name: "Paper Review",
          input_name: "paper.pdf",
          input_mode: "document",
          input_interpretation: "document",
          step_id: "synthesis",
          step_label: "Synthesis",
          title: "Identification assumption is unstated",
          severity: "high",
          section: "Model",
          body: "The identifying restriction is used but never stated.",
          evidence: [
            {
              page: 7,
              node_id: "paragraph-12",
              description: "Model statement",
            },
          ],
        },
      ],
    },
    {
      id: "issue-code",
      title: "Retry loop has no bound",
      severity: "medium",
      section: "src/worker.rs",
      status: "regressed",
      note: "Reappeared after the refactor.",
      created: "2026-08-03T00:00:00Z",
      updated: "2026-08-09T00:00:00Z",
      decision_updated: "2026-08-09T00:00:00Z",
      occurrences: [
        {
          key: "occ-code",
          run_id: "run_code",
          issue_id: "RETRY-1",
          observed_at: "2026-08-09T00:00:00Z",
          profile_id: "code-review",
          profile_name: "Code Review",
          input_name: "src",
          input_mode: "folder",
          input_interpretation: "source_tree",
          step_id: "synthesis",
          step_label: "Synthesis",
          title: "Retry loop has no bound",
          severity: "medium",
          section: "src/worker.rs",
          body: "The worker retries permanently after a terminal response.",
          evidence: [
            {
              artifact_path: "artifacts/source/src/worker.rs",
              line_start: 41,
              line_end: 45,
              description: "Retry implementation",
            },
          ],
        },
      ],
    },
  ],
};

describe("ProjectIssueLedgerPanel", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation(
      (command: string, args?: { status?: string; note?: string }) => {
        if (command === "sync_project_issue_ledger")
          return Promise.resolve(ledger);
        if (command === "update_project_issue") {
          return Promise.resolve({
            ...ledger,
            issues: ledger.issues.map((issue) =>
              issue.id === "issue-paper"
                ? {
                    ...issue,
                    status:
                      args?.status === "automatic" ? "open" : args?.status,
                    note: args?.note ?? "",
                  }
                : issue,
            ),
          });
        }
        return Promise.reject(new Error(`unexpected command: ${command}`));
      },
    );
  });

  it("tracks generic report findings and opens cited source evidence", async () => {
    const user = userEvent.setup();
    const onOpenRun = vi.fn();
    render(
      <ProjectIssueLedgerPanel
        project={project}
        runs={runs}
        onOpenRun={onOpenRun}
      />,
    );

    expect(
      await screen.findByText("Identification assumption is unstated"),
    ).toBeVisible();
    await user.click(
      screen.getByRole("button", {
        name: "Review Identification assumption is unstated",
      }),
    );
    await user.click(
      screen.getByRole("button", { name: "Open Page 7 from Paper review" }),
    );
    expect(onOpenRun).toHaveBeenCalledWith("run_paper", {
      page: 7,
      relPath: undefined,
    });

    await user.selectOptions(
      screen.getByRole("combobox", {
        name: "Lifecycle for Identification assumption is unstated",
      }),
      "addressed",
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("update_project_issue", {
        projectId: project.id,
        issueId: "issue-paper",
        status: "addressed",
        note: "",
      }),
    );
  });

  it("filters the ledger by workflow and revision", async () => {
    const user = userEvent.setup();
    render(
      <ProjectIssueLedgerPanel
        project={project}
        runs={runs}
        onOpenRun={() => {}}
      />,
    );
    await screen.findByText("Identification assumption is unstated");

    await user.selectOptions(
      screen.getByRole("combobox", { name: "Issue review type filter" }),
      "code-review",
    );
    expect(screen.getByText("Retry loop has no bound")).toBeVisible();
    expect(
      screen.queryByText("Identification assumption is unstated"),
    ).not.toBeInTheDocument();

    await user.selectOptions(
      screen.getByRole("combobox", { name: "Issue report filter" }),
      "run_paper",
    );
    expect(screen.getByText("No issues match these filters.")).toBeVisible();
  });

  it("shows the merged note on the target and keeps it on the next lifecycle change", async () => {
    const user = userEvent.setup();
    const mergedNote = "Reappeared after the refactor.";
    const merged = {
      ...ledger,
      issues: [
        {
          ...ledger.issues[0],
          note: mergedNote,
          occurrences: [
            ...ledger.issues[0].occurrences,
            ...ledger.issues[1].occurrences,
          ],
        },
      ],
    };
    invoke.mockImplementation(
      (command: string, args?: { status?: string; note?: string }) => {
        if (command === "sync_project_issue_ledger")
          return Promise.resolve(ledger);
        if (command === "merge_project_issues") return Promise.resolve(merged);
        if (command === "update_project_issue")
          return Promise.resolve({
            ...merged,
            issues: [
              {
                ...merged.issues[0],
                status: args?.status,
                note: args?.note?.trim() ?? "",
              },
            ],
          });
        return Promise.reject(new Error(`unexpected command: ${command}`));
      },
    );
    render(
      <ProjectIssueLedgerPanel
        project={project}
        runs={runs}
        onOpenRun={() => {}}
      />,
    );
    await user.click(
      await screen.findByRole("button", {
        name: "Review Retry loop has no bound",
      }),
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Combine duplicate into" }),
      "issue-paper",
    );
    confirm.mockResolvedValueOnce(false);
    await user.click(screen.getByRole("button", { name: "Combine" }));
    await waitFor(() => expect(confirm).toHaveBeenCalledOnce());
    expect(confirm.mock.calls[0][0]).toContain("can't be undone");
    expect(invoke).not.toHaveBeenCalledWith(
      "merge_project_issues",
      expect.anything(),
    );

    confirm.mockResolvedValueOnce(true);
    await user.click(screen.getByRole("button", { name: "Combine" }));

    const note = await screen.findByRole("textbox", {
      name: "Collection note or decision rationale",
    });
    expect(note).toHaveValue(mergedNote);
    expect(screen.getByRole("button", { name: "Save note" })).toBeDisabled();

    await user.selectOptions(
      screen.getByRole("combobox", {
        name: "Lifecycle for Identification assumption is unstated",
      }),
      "addressed",
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("update_project_issue", {
        projectId: project.id,
        issueId: "issue-paper",
        status: "addressed",
        note: mergedNote,
      }),
    );
  });

  it("resets a saved draft to the stored note so Save note turns off", async () => {
    const user = userEvent.setup();
    render(
      <ProjectIssueLedgerPanel
        project={project}
        runs={runs}
        onOpenRun={() => {}}
      />,
    );
    await user.click(
      await screen.findByRole("button", {
        name: "Review Identification assumption is unstated",
      }),
    );
    const note = screen.getByRole("textbox", {
      name: "Collection note or decision rationale",
    });
    await user.type(note, "Check the appendix.  ");
    invoke.mockImplementationOnce(
      (_command: string, args?: { status?: string; note?: string }) =>
        Promise.resolve({
          ...ledger,
          issues: ledger.issues.map((issue) =>
            issue.id === "issue-paper"
              ? { ...issue, note: args?.note?.trim() ?? "" }
              : issue,
          ),
        }),
    );
    await user.click(screen.getByRole("button", { name: "Save note" }));

    await waitFor(() => expect(note).toHaveValue("Check the appendix."));
    expect(screen.getByRole("button", { name: "Save note" })).toBeDisabled();
  });

  it("ignores a slow ledger refresh for a collection that is no longer selected", async () => {
    let resolveFirst: (value: unknown) => void = () => {};
    const otherProject = {
      ...project,
      id: "other-project",
      name: "Other project",
      run_ids: ["run_code"],
    };
    const otherLedger = {
      ...ledger,
      project_id: otherProject.id,
      issues: [ledger.issues[1]],
    };
    invoke.mockImplementation(
      (command: string, args?: { projectId?: string }) => {
        if (command !== "sync_project_issue_ledger")
          return Promise.reject(new Error(`unexpected command: ${command}`));
        if (args?.projectId === project.id)
          return new Promise((resolve) => {
            resolveFirst = resolve;
          });
        return Promise.resolve(otherLedger);
      },
    );
    const { rerender } = render(
      <ProjectIssueLedgerPanel
        project={project}
        runs={runs}
        onOpenRun={() => {}}
      />,
    );
    rerender(
      <ProjectIssueLedgerPanel
        project={otherProject}
        runs={runs}
        onOpenRun={() => {}}
      />,
    );
    expect(await screen.findByText("Retry loop has no bound")).toBeVisible();

    await act(async () => resolveFirst(ledger));
    expect(screen.getByRole("button", { name: "Refresh" })).toBeEnabled();
    expect(
      screen.queryByText("Identification assumption is unstated"),
    ).not.toBeInTheDocument();
  });

  it("shows a failed sync without claiming there is nothing to show", async () => {
    invoke.mockImplementation(() => Promise.reject(new Error("Sync failed")));
    render(
      <ProjectIssueLedgerPanel
        project={project}
        runs={runs}
        onOpenRun={() => {}}
      />,
    );
    await screen.findByText("Sync failed");
    expect(screen.queryByText(/No structured issues/)).toBeNull();
  });

  it("starts each collection with every filter cleared", async () => {
    const user = userEvent.setup();
    const view = (id: string) => (
      <ProjectIssueLedgerPanel
        project={{ ...project, id }}
        runs={runs}
        onOpenRun={() => {}}
      />
    );
    const page = render(view(project.id));
    await screen.findByText("Identification assumption is unstated");
    const severity = () =>
      screen.getByRole("combobox", { name: "Issue severity filter" });
    const search = () => screen.getByLabelText("Search collection issues");
    await user.selectOptions(severity(), "high");
    await user.type(search(), "retry");

    page.rerender(view("another-collection"));
    await waitFor(() => expect(severity()).toHaveValue("all"));
    expect(search()).toHaveValue("");
  });
});
