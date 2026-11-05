import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { appEvents } from "../../lib/appEvents";
import { projectClient } from "../../lib/projectClient";
import { workspaceClient } from "../../lib/workbench/workspaceClient";
import { useProjectSurfaceData } from "./useProjectSurfaceData";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => vi.fn()),
}));
vi.mock("../../lib/projectClient", () => ({
  projectClient: { home: vi.fn(), mutate: vi.fn() },
}));
vi.mock("../../lib/workbench/workspaceClient", () => ({
  workspaceClient: { getWorkspace: vi.fn() },
}));

it("refreshes a mounted review surface when opened from an automatic copy", async () => {
  const home = { settings: { body: {} }, papers: [], tasks: [] };
  vi.mocked(projectClient.home).mockResolvedValue(home as never);
  vi.mocked(workspaceClient.getWorkspace).mockResolvedValue({
    id: "project",
    root: null,
  } as never);
  const { result } = renderHook(() =>
    useProjectSurfaceData("project", vi.fn()),
  );
  await waitFor(() => expect(result.current.data).toEqual(home));
  const updated = { ...home, tasks: [{ id: "automatic-copy" }] };
  vi.mocked(projectClient.home).mockResolvedValue(updated as never);
  act(() =>
    appEvents.emit("workspace-destination", {
      workspaceId: "project",
      tab: "edits",
    }),
  );
  await waitFor(() => expect(result.current.data).toEqual(updated));
  expect(projectClient.mutate).not.toHaveBeenCalled();
});
