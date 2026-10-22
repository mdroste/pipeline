import { renderHook, act, waitFor } from "@testing-library/react";
import { it, expect, vi } from "vitest";
import { useWorkspacePageController } from "./useWorkspacePageController";
const mocks = vi.hoisted(() => ({
  createSession: vi.fn(),
  resolveServerRequest: vi.fn(),
  listWorkspaces: vi.fn(),
  pendingRequests: vi.fn(),
  connectCodex: vi.fn(),
  accountState: vi.fn(),
  listSessions: vi.fn(),
  conversationSnapshot: vi.fn(),
  sessionSnapshot: vi.fn(),
  reconcileSession: vi.fn(),
  updateSession: vi.fn(),
  sendTurn: vi.fn(),
  listeners: [] as any[],
}));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_: string, cb: any) => {
    mocks.listeners.push(cb);
    return () => {};
  }),
}));
async function draftFixture(initial: Record<string, unknown> = {}) {
  vi.clearAllMocks();
  mocks.listeners.length = 0;
  localStorage.clear();
  localStorage.setItem("pipeline.workspace.sessionId", "A");
  let saved: any = {
    workspace: null,
    session: {
      id: "A",
      title: "A",
      workspaceId: null,
      paperId: null,
      presetId: null,
      overrides: {},
      draft: "",
      revision: 1,
      archivedAt: null,
      createdAt: "now",
      updatedAt: "now",
    },
    sequence: 1,
    activeBinding: null,
    turns: [],
    items: [],
    ...initial,
  };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [] });
  mocks.pendingRequests.mockResolvedValue([]);
  mocks.connectCodex.mockResolvedValue({});
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.listSessions.mockImplementation(async () => ({
    sessions: [saved.session],
  }));
  mocks.conversationSnapshot.mockImplementation(async () => saved);
  mocks.sessionSnapshot.mockImplementation(async () => saved);
  mocks.reconcileSession.mockResolvedValue(false);
  mocks.updateSession.mockImplementation(async (r: any) => {
    saved = {
      ...saved,
      session: { ...saved.session, ...r, revision: saved.session.revision + 1 },
      sequence: saved.sequence + 1,
    };
    return { record: saved.session, sequence: saved.sequence };
  });
  const hook = renderHook(() => useWorkspacePageController({}));
  await waitFor(() =>
    expect(hook.result.current.snapshot?.session.id).toBe("A"),
  );
  return { ...hook, saved: () => saved };
}
it("a failed request resolution recovers the still-pending approval", async () => {
  const { result, unmount } = await draftFixture({
    activeBinding: { id: "binding", providerThreadId: "thread-A" },
  });
  const request = {
    epoch: 1,
    kind: "serverRequest",
    requestId: 5,
    method: "item/commandExecution/requestApproval",
    params: { threadId: "thread-A" },
  };
  await act(async () => {
    for (const cb of mocks.listeners) cb({ payload: request });
  });
  expect(result.current.visibleRequests).toHaveLength(1);
  mocks.pendingRequests.mockResolvedValue([request]);
  mocks.resolveServerRequest.mockRejectedValue(
    new Error("temporary IPC error"),
  );
  await act(async () => {
    await result.current.resolve(request as any, { decision: "accept" });
  });
  expect(result.current.error).toContain("temporary IPC error");
  expect(result.current.visibleRequests).toHaveLength(1);
  expect(mocks.pendingRequests).toHaveBeenCalledTimes(2);
  unmount();
});
it("late conversation creation respects newer project navigation", async () => {
  const { result, saved, unmount } = await draftFixture();
  let finish!: (v: any) => void;
  mocks.createSession.mockImplementationOnce(
    () =>
      new Promise((r) => {
        finish = r;
      }),
  );
  let creating!: Promise<void>;
  act(() => {
    creating = result.current.createSession();
  });
  await waitFor(() => expect(mocks.createSession).toHaveBeenCalled());
  const workspaceB = { id: "project-B", name: "Project B" };
  const snapB: any = {
    ...saved(),
    workspace: workspaceB,
    session: {
      ...saved().session,
      id: "B",
      workspaceId: "project-B",
      draft: "B",
    },
  };
  const created: any = {
    ...saved(),
    session: {
      ...saved().session,
      id: "created-in-unfiled",
      workspaceId: null,
      draft: "",
    },
  };
  mocks.listSessions.mockImplementation(async (workspace: string | null) => ({
    sessions: workspace === "project-B" ? [snapB.session] : [created.session],
  }));
  mocks.conversationSnapshot.mockImplementation(async (id: string) =>
    id === "B" ? snapB : id === created.session.id ? created : saved(),
  );
  mocks.sessionSnapshot.mockImplementation(async (id: string) =>
    id === "B" ? snapB : saved(),
  );
  await act(async () => {
    await result.current.selectProject("project-B");
  });
  await waitFor(() => expect(result.current.snapshot?.session.id).toBe("B"));
  await act(async () => {
    finish({ record: created.session });
    await creating;
  });
  await waitFor(() => expect(result.current.snapshot?.session.id).toBe("B"));
  expect(result.current.workspaceId).toBe("project-B");
  expect(result.current.snapshot?.session.workspaceId).toBe("project-B");
  unmount();
});

it("deduplicates pending approval decisions and reconciles ambiguous acknowledgements", async () => {
  const { result, unmount } = await draftFixture({
    activeBinding: { id: "binding", providerThreadId: "thread-A" },
  });
  const request: any = {
    epoch: 1,
    kind: "serverRequest",
    requestId: 5,
    method: "item/commandExecution/requestApproval",
    params: { threadId: "thread-A" },
  };
  let reject!: (e: Error) => void;
  mocks.resolveServerRequest.mockImplementationOnce(
    () =>
      new Promise((_, no) => {
        reject = no;
      }),
  );
  let pending!: Promise<void>;
  act(() => {
    pending = result.current.resolve(request, { decision: "accept" });
  });
  await act(async () => {
    await result.current.resolve(request, { decision: "accept" });
  });
  expect(mocks.resolveServerRequest).toHaveBeenCalledTimes(1);
  mocks.pendingRequests.mockRejectedValueOnce(new Error("offline"));
  await act(async () => {
    reject(new Error("acknowledgement lost"));
    await pending;
  });
  mocks.pendingRequests.mockResolvedValue([]); // Server consumed the first decision.
  await act(async () => {
    await result.current.resolve(request, { decision: "accept" });
  });
  expect(mocks.resolveServerRequest).toHaveBeenCalledTimes(1);
  unmount();
});
