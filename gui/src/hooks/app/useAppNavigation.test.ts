import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { loadDeskLayout } from "../../lib/deskLayout";
import { router } from "../../lib/router";
import type { ArtifactSelectionTarget } from "../../lib/artifactTypes";
import { useAppNavigation, useAppRouteCommits } from "./useAppNavigation";

const confirmDialog = vi.hoisted(() => vi.fn());
const createSession = vi.hoisted(() => vi.fn());
vi.mock("../../components/DialogService", () => ({ confirmDialog }));
vi.mock("../../lib/workbench/conversationClient", () => ({
  conversationClient: { createSession },
}));
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const pageTwelve: ArtifactSelectionTarget = { page: 12 };

function renderNavigation() {
  // What a history destination would receive on each render.
  const seen: { route: string; source: ArtifactSelectionTarget | null }[] = [];
  const hook = renderHook(() => {
    const navigation = useAppNavigation();
    useAppRouteCommits(navigation, vi.fn().mockResolvedValue(null), vi.fn());
    seen.push({
      route: `${navigation.page}:${navigation.historyRunId ?? ""}`,
      source: navigation.historySourceSelection,
    });
    return navigation;
  });
  return { ...hook, seen };
}

beforeEach(() => {
  localStorage.clear();
  confirmDialog.mockReset();
  createSession.mockReset();
  createSession.mockResolvedValue({ record: { id: "created" } });
});

it("opens a saved run on its cited source from the first render", async () => {
  const { result, seen } = renderNavigation();
  await act(() => router.navigate({ page: "projects" }));

  await act(() => result.current.openHistoryRun("run-a", pageTwelve));
  expect(router.route).toEqual({ page: "history", runId: "run-a" });
  expect(result.current.historySourceSelection).toEqual(pageTwelve);
  // The report never renders without the source first.
  const runRenders = seen.filter((entry) => entry.route === "history:run-a");
  expect(runRenders.length).toBeGreaterThan(0);
  expect(runRenders.every((entry) => entry.source === pageTwelve)).toBe(true);

  // Another run, opened without a source, does not inherit this one.
  await act(() => router.navigate({ page: "history", runId: "run-b" }));
  expect(result.current.historySourceSelection).toBeNull();

  // Leaving History ends the visit; reopening the run plainly has no source.
  await act(() => router.navigate({ page: "projects" }));
  await act(() => router.navigate({ page: "history", runId: "run-a" }));
  expect(result.current.historySourceSelection).toBeNull();
});

it("restores the earlier source when leaving the current page is declined", async () => {
  const { result } = renderNavigation();
  await act(() => router.navigate({ page: "tasks" }));
  act(() => result.current.setTasksDirty(true));
  confirmDialog.mockResolvedValue(false);

  await act(() => result.current.openHistoryRun("run-a", pageTwelve));
  expect(router.route).toEqual({ page: "tasks" });

  // A later plain open of the same run must not pick up the declined source.
  confirmDialog.mockResolvedValue(true);
  await act(() => router.navigate({ page: "history", runId: "run-a" }));
  expect(result.current.historySourceSelection).toBeNull();
});

it("forgets a workflow's recovery copy when its edits are discarded on leaving", async () => {
  localStorage.setItem("pipeline.workflowDraft.grant", "{}");
  invoke.mockResolvedValue("grant");
  const { result } = renderNavigation();
  await act(() => router.navigate({ page: "pipeline" }));
  act(() => result.current.setWorkflowDirty(true));

  confirmDialog.mockResolvedValueOnce(false);
  await act(() => router.navigate({ page: "main" }));
  expect(router.route.page).toBe("pipeline");
  expect(localStorage.getItem("pipeline.workflowDraft.grant")).toBe("{}");

  confirmDialog.mockResolvedValueOnce(true);
  await act(() => router.navigate({ page: "main" }));
  expect(router.route.page).toBe("main");
  expect(localStorage.getItem("pipeline.workflowDraft.grant")).toBeNull();
});

it("creates no conversation when leaving unsaved work is declined", async () => {
  const { result } = renderNavigation();
  await act(() => router.navigate({ page: "tasks" }));
  act(() => result.current.setTasksDirty(true));
  confirmDialog.mockResolvedValue(false);

  await act(() => result.current.startConversation());
  expect(createSession).not.toHaveBeenCalled();
  expect(router.route).toEqual({ page: "tasks" });
});

it("asks once and opens the new conversation when leaving is accepted", async () => {
  const { result } = renderNavigation();
  await act(() => router.navigate({ page: "tasks" }));
  act(() => result.current.setTasksDirty(true));
  confirmDialog.mockResolvedValue(true);

  await act(() => result.current.startConversation());
  expect(confirmDialog).toHaveBeenCalledOnce();
  expect(router.route).toEqual({
    page: "workspace",
    sessionId: "created",
    surface: "chat",
  });
});

it("leaves a project's saved tab alone when opening its conversation", async () => {
  renderNavigation();
  localStorage.setItem(
    "pipeline.desk.project",
    JSON.stringify({ tab: "plans", object: null }),
  );
  await act(() =>
    router.navigate({
      page: "workspace",
      projectId: "project",
      sessionId: "conversation",
      surface: "chat",
    }),
  );
  expect(loadDeskLayout("project").tab).toBe("plans");

  // A route that names no conversation still opens where it says.
  await act(() => router.navigate({ page: "workspace", projectId: "project" }));
  expect(loadDeskLayout("project").tab).toBe("overview");
});
