import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import useProjectSignals from "./useProjectSignals";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

it("asks for project conversations before the automation page is selected", async () => {
  const own = {
    id: "older-project-result",
    sessionId: "project-chat",
    state: "finished",
  };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "task_list")
      return (args as { sessionIds?: string[] }).sessionIds?.includes(
        "project-chat",
      )
        ? [own]
        : [];
    return [];
  });
  const { result } = renderHook(() =>
    useProjectSignals({
      workspaceId: "project",
      root: null,
      sessionIds: ["project-chat"],
      enabled: true,
      stamp: 1,
    }),
  );
  await waitFor(() =>
    expect(result.current.signals.automations).toEqual([own]),
  );
  expect(invoke).toHaveBeenCalledWith("task_list", {
    view: "projectActivity",
    sessionIds: ["project-chat"],
    offset: 0,
  });
});

it("loads all four linked collections and reports a failed subset without claiming zero", async () => {
  const collections = ["one", "two", "three", "four"].map((id) => ({
    id,
    name: id,
    run_ids: ["run"],
  }));
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "list_runs")
      return [{ run_id: "run", input_path: "/project/paper.tex" }];
    if (command === "list_projects")
      return { projects: collections, warnings: [] };
    if (command === "sync_project_issue_ledger") {
      const id = (args as { projectId: string }).projectId;
      if (id === "two") throw new Error("Offline");
      return { project_id: id, issues: [] };
    }
    return [];
  });
  const { result } = renderHook(() =>
    useProjectSignals({
      workspaceId: "p",
      root: "/project",
      sessionIds: [],
      enabled: true,
      stamp: 1,
    }),
  );
  await waitFor(() =>
    expect(result.current.signals.sources?.reviews?.state).toBe("ready"),
  );
  expect(result.current.signals.ledgers).toHaveLength(3);
  expect(result.current.signals.sources?.reviews).toMatchObject({
    complete: false,
    error: expect.stringContaining("3 of 4"),
  });
  expect(
    result.current.signals.ledgers?.some(
      (ledger) => ledger.project_id === "four",
    ),
  ).toBe(true);
  vi.mocked(invoke).mockRejectedValue(new Error("Still offline"));
  act(() => result.current.reload("reviews"));
  await waitFor(() =>
    expect(result.current.signals.sources?.reviews?.state).toBe("error"),
  );
  expect(result.current.signals.ledgers).toHaveLength(3);
  expect(result.current.signals.sources?.reviews?.fetchedAt).toBeTruthy();
});

it("keeps an entirely failed set of collections explicitly incomplete", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "list_runs")
      return [{ run_id: "run", input_path: "/project/paper.tex" }];
    if (command === "list_projects")
      return {
        projects: [{ id: "one", name: "Review", run_ids: ["run"] }],
        warnings: [],
      };
    if (command === "sync_project_issue_ledger") throw new Error("Offline");
    return [];
  });
  const { result } = renderHook(() =>
    useProjectSignals({
      workspaceId: "p",
      root: "/project",
      sessionIds: [],
      enabled: true,
      stamp: 1,
    }),
  );
  await waitFor(() =>
    expect(result.current.signals.sources?.reviews?.state).toBe("ready"),
  );
  expect(result.current.signals.sources?.reviews).toMatchObject({
    complete: false,
    error: expect.stringContaining("0 of 1"),
  });
});
