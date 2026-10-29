import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import ProjectReviews from "./ProjectReviews";
import { HistoryRunOpener } from "../../hooks/app/historyRunOpener";
import type { ArtifactSelectionTarget } from "../../lib/artifactTypes";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

const pageTwelve: ArtifactSelectionTarget = { page: 12 };

// The ledger's own rendering is covered elsewhere; here it only reports which
// run (and which cited source) the researcher asked to open.
vi.mock("../ProjectIssueLedgerPanel", () => ({
  default: ({
    onOpenRun,
  }: {
    onOpenRun: (runId: string, source?: ArtifactSelectionTarget) => void;
  }) => (
    <button type="button" onClick={() => onOpenRun("in-project", pageTwelve)}>
      Page 12
    </button>
  ),
}));

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (command: string) => {
    if (command === "list_runs")
      return [
        {
          run_id: "in-project",
          created: "2026-09-28T21:14:00Z",
          input_name: "draft_v7.pdf",
          input_path: "/papers/minwage/draft_v7.pdf",
          profile_name: "Auto Paper Review",
          status: "complete",
          title: "Review of draft_v7.pdf",
        },
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
        ],
        warnings: [],
      };
    throw new Error(`unexpected ${command}`);
  });
});

it("opens evidence at the cited source and plain runs without one", async () => {
  const openHistoryRun = vi.fn();
  const user = userEvent.setup();
  render(
    <HistoryRunOpener.Provider value={openHistoryRun}>
      <ProjectReviews workspaceRoot="/papers/minwage" />
    </HistoryRunOpener.Provider>,
  );

  await user.click(await screen.findByRole("button", { name: "Page 12" }));
  expect(openHistoryRun).toHaveBeenLastCalledWith("in-project", pageTwelve);

  await user.click(screen.getByRole("button", { name: /Review of draft_v7/ }));
  expect(openHistoryRun).toHaveBeenLastCalledWith("in-project", null);
});
