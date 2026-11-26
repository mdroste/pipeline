import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import useSearchSources from "./useSearchSources";
const mocks = vi.hoisted(() => ({
  projects: vi.fn(),
  reviews: vi.fn(),
  tasks: vi.fn(),
  task: vi.fn(),
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { listWorkspaces: mocks.projects },
}));
vi.mock("../lib/reviewHistoryClient", () => ({
  reviewHistoryClient: { list: mocks.reviews },
}));
vi.mock("../lib/taskClient", () => ({
  taskClient: { list: mocks.tasks, get: mocks.task },
}));
beforeEach(() => {
  vi.clearAllMocks();
  mocks.projects.mockResolvedValue({ workspaces: [] });
  mocks.reviews.mockResolvedValue([]);
  mocks.tasks.mockResolvedValue([{ id: "t", name: "Citation check" }]);
  mocks.task.mockResolvedValue({ scope: { workspaceId: "p" } });
});
it("includes finished step automations, discloses pagination, and loads the next page", async () => {
  const rows = Array.from({ length: 53 }, (_, i) => ({
    id: String(i),
    name: `Automation ${i}`,
  }));
  mocks.tasks.mockImplementation((_view, _session, offset) =>
    Promise.resolve(rows.slice(offset, offset + 51)),
  );
  const { result } = renderHook(useSearchSources);
  await waitFor(() => expect(result.current.states.Automations).toBe("ready"));
  expect(mocks.tasks).toHaveBeenCalledWith("session", null, 0, true);
  expect(result.current.tasks).toHaveLength(50);
  expect(result.current.moreTasks).toBe(true);
  expect(result.current.tasks[0].projectId).toBe("p");
  act(() => result.current.loadMoreTasks());
  await waitFor(() => expect(result.current.tasks).toHaveLength(53));
  expect(new Set(result.current.tasks.map((task) => task.id)).size).toBe(53);
  expect(result.current.moreTasks).toBe(false);
});
it("keeps names when project associations fail and retries the missing scope", async () => {
  mocks.task.mockRejectedValueOnce(new Error("unavailable"));
  const { result } = renderHook(useSearchSources);
  await waitFor(() => expect(result.current.states.Automations).toBe("ready"));
  expect(result.current.tasks[0].name).toBe("Citation check");
  expect(result.current.taskNotices[0]).toMatch(
    /project association unavailable/,
  );
  act(() => result.current.retry());
  await waitFor(() => expect(result.current.tasks[0].projectId).toBe("p"));
  expect(result.current.taskNotices).toEqual([]);
});
