import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import {
  useWorkspaceRouteEntry,
  type WorkspaceEntryTarget,
} from "./useWorkspaceRouteEntry";
it("only the latest asynchronous project or conversation entry can commit", async () => {
  const resolvers: Record<string, () => void> = {};
  const commits: string[] = [];
  const errors = vi.fn();
  const apply = async (
    target: WorkspaceEntryTarget,
    current: () => boolean,
  ) => {
    const id = target.sessionId!;
    await new Promise<void>((resolve) => {
      resolvers[id] = resolve;
    });
    if (current()) commits.push(id);
  };
  const { rerender, unmount } = renderHook(
    ({ target, request }) =>
      useWorkspaceRouteEntry(request, target, apply, errors),
    {
      initialProps: {
        request: 1,
        target: { page: "workspace", sessionId: "A" } as WorkspaceEntryTarget,
      },
    },
  );
  rerender({ request: 2, target: { page: "workspace", sessionId: "B" } });
  rerender({ request: 3, target: { page: "workspace", sessionId: "C" } });
  await act(async () => {
    resolvers.C();
    resolvers.B();
    resolvers.A();
  });
  await waitFor(() => expect(commits).toEqual(["C"]));
  rerender({ request: 4, target: { page: "workspace", sessionId: "D" } });
  unmount();
  await act(async () => resolvers.D());
  expect(commits).toEqual(["C"]);
  expect(errors).not.toHaveBeenCalled();
});
