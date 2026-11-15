import { renderHook, waitFor } from "@testing-library/react";
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
