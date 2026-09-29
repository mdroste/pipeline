import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ProjectActionItems from "./ProjectActionItems";
import type { SurfaceApi } from "./shared";
import type {
  ProjectHome,
  ProjectRecord,
  ResearchTask,
} from "../../lib/projectClient";
const mocks = vi.hoisted(() => ({ taskPage: vi.fn() }));
vi.mock("../../lib/projectClient", () => ({ projectClient: mocks }));
const older: ProjectRecord<ResearchTask> = {
  id: "old",
  workspaceId: "A",
  kind: "task",
  revision: 1,
  updatedAt: "old",
  body: {
    objective: "Older open action",
    status: "open",
    anchorId: null,
    expectedOutputs: [],
    expectedChecks: [],
  },
};
const props = (): SurfaceApi =>
  ({
    workspaceId: "A",
    workspace: { root: "/project" },
    data: {
      tasks: [],
      tasksCursor: { id: "cursor", updatedAt: "now" },
      anchors: [],
      fileAcceptance: true,
    },
    run: vi.fn(),
    setTaskId: vi.fn(),
    setTab: vi.fn(),
    act: vi.fn(),
  }) as unknown as SurfaceApi;
it("loads older action items and carries the selected record into the edit view", async () => {
  const api = props();
  mocks.taskPage.mockResolvedValue({ records: [older], nextCursor: null });
  render(<ProjectActionItems {...api} />);
  fireEvent.click(
    screen.getByRole("button", { name: "Show older action items" }),
  );
  await screen.findByText("Older open action");
  fireEvent.click(screen.getByRole("button", { name: "Start an edit" }));
  expect(api.setTaskId).toHaveBeenCalledWith("old", older);
  expect(api.setTab).toHaveBeenCalledWith("edits");
  expect(
    screen.queryByRole("button", { name: "Show older action items" }),
  ).toBeNull();
});
it("discards a page response after the project changes", async () => {
  let resolve!: (v: unknown) => void;
  mocks.taskPage.mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const api = props();
  const view = render(<ProjectActionItems {...api} />);
  fireEvent.click(
    screen.getByRole("button", { name: "Show older action items" }),
  );
  view.rerender(
    <ProjectActionItems
      {...api}
      workspaceId="B"
      data={{ ...api.data, tasks: [], tasksCursor: null } as ProjectHome}
    />,
  );
  await act(async () => resolve({ records: [older], nextCursor: null }));
  expect(screen.queryByText("Older open action")).toBeNull();
});
