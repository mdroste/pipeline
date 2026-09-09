import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ProjectIssueLedgerPanel from "./ProjectIssueLedgerPanel";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
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
      screen.getByRole("combobox", { name: "Issue workflow filter" }),
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
});
