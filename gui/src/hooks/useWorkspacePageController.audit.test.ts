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
vi.mock("../lib/workbench/workspaceClient", () => ({ workspaceClient: mocks }));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: mocks,
}));
vi.mock("../lib/workbench/runtimeClient", () => ({ runtimeClient: mocks }));
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
    expect(hook.result.current.conversation.snapshot?.session.id).toBe("A"),
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
  expect(result.current.conversation.visibleRequests).toHaveLength(1);
  mocks.pendingRequests.mockResolvedValue([request]);
  mocks.resolveServerRequest.mockRejectedValue(
    new Error("temporary IPC error"),
  );
  await act(async () => {
    await result.current.conversation.resolve(request as any, {
      decision: "accept",
    });
  });
  expect(result.current.feedback.error).toContain("temporary IPC error");
  expect(result.current.conversation.visibleRequests).toHaveLength(1);
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
    creating = result.current.navigation.createSession();
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
    await result.current.navigation.selectProject("project-B");
  });
  await waitFor(() =>
    expect(result.current.conversation.snapshot?.session.id).toBe("B"),
  );
  await act(async () => {
    finish({ record: created.session });
    await creating;
  });
  await waitFor(() =>
    expect(result.current.conversation.snapshot?.session.id).toBe("B"),
  );
  expect(result.current.navigation.workspaceId).toBe("project-B");
  expect(result.current.conversation.snapshot?.session.workspaceId).toBe(
    "project-B",
  );
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
    pending = result.current.conversation.resolve(request, {
      decision: "accept",
    });
  });
  await act(async () => {
    await result.current.conversation.resolve(request, { decision: "accept" });
  });
  expect(mocks.resolveServerRequest).toHaveBeenCalledTimes(1);
  mocks.pendingRequests.mockRejectedValueOnce(new Error("offline"));
  await act(async () => {
    reject(new Error("acknowledgement lost"));
    await pending;
  });
  mocks.pendingRequests.mockResolvedValue([]); // Server consumed the first decision.
  await act(async () => {
    await result.current.conversation.resolve(request, { decision: "accept" });
  });
  expect(mocks.resolveServerRequest).toHaveBeenCalledTimes(1);
  unmount();
});

const emit = async (payload: Record<string, unknown>) => {
  await act(async () => {
    for (const cb of mocks.listeners) cb({ payload });
  });
};
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const binding = { id: "binding", providerThreadId: "thread-A" };
async function sendAcknowledged(
  result: { current: ReturnType<typeof useWorkspacePageController> },
  text = "Question",
) {
  mocks.sendTurn.mockResolvedValueOnce({
    epoch: 1,
    threadId: "thread-A",
    turnId: "turn-1",
  });
  act(() => result.current.persistence.editDraft(text));
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(result.current.conversation.active?.turnId).toBe("turn-1");
}
const stream = async (itemId: string, delta: string) => {
  await emit({
    epoch: 1,
    kind: "agentMessageDelta",
    threadId: "thread-A",
    turnId: "turn-1",
    itemId,
    delta,
  });
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 80));
  });
};
const recordedTurn = (saved: any, answer: string) => ({
  ...saved,
  sequence: saved.sequence + 1,
  turns: [
    {
      id: "turn-record",
      bindingId: "binding",
      providerTurnId: "turn-1",
      terminalAt: "now",
      state: "done",
    },
  ],
  items: [
    {
      id: "user-item",
      turnId: "turn-record",
      providerItemId: "user-1",
      itemKind: "userMessage",
      payload: { text: "Question" },
      isFinal: true,
      createdAt: "now",
      updatedAt: "now",
    },
    {
      id: "agent-item",
      turnId: "turn-record",
      providerItemId: "agent-1",
      itemKind: "agentMessage",
      payload: { text: answer },
      isFinal: true,
      createdAt: "now",
      updatedAt: "now",
    },
  ],
});

it("a slow reconcile of a conversation the user left neither blanks nor reports into the next one", async () => {
  vi.clearAllMocks();
  mocks.listeners.length = 0;
  localStorage.clear();
  localStorage.setItem("pipeline.workspace.sessionId", "A");
  const make = (id: string) => ({
    workspace: null,
    session: {
      id,
      title: id,
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
  });
  const reconcileA = deferred<boolean>();
  const snapshotB = deferred<any>();
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [] });
  mocks.pendingRequests.mockResolvedValue([]);
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.listSessions.mockResolvedValue({
    sessions: [make("A").session, make("B").session],
  });
  mocks.reconcileSession.mockImplementation((id: string) =>
    id === "A" ? reconcileA.promise : Promise.resolve(false),
  );
  mocks.conversationSnapshot.mockImplementation((id: string) =>
    id === "B" ? snapshotB.promise : Promise.resolve(make(id)),
  );
  const { result, unmount } = renderHook(() => useWorkspacePageController({}));
  await waitFor(() => expect(mocks.reconcileSession).toHaveBeenCalledWith("A"));
  await act(async () => await result.current.navigation.selectSession("B"));
  await waitFor(() =>
    expect(mocks.conversationSnapshot).toHaveBeenCalledWith("B"),
  );
  await act(async () => {
    reconcileA.reject(new Error("thread resume failed"));
    await Promise.resolve();
  });
  await act(async () => {
    snapshotB.resolve(make("B"));
  });
  await waitFor(() =>
    expect(result.current.conversation.snapshot?.session.id).toBe("B"),
  );
  expect(mocks.conversationSnapshot).not.toHaveBeenCalledWith("A");
  expect(result.current.feedback.error).toBeNull();
  unmount();
});

it("keeps the finished prompt and answer on screen until the refreshed transcript arrives", async () => {
  const { result, saved, unmount } = await draftFixture({
    activeBinding: binding,
  });
  await sendAcknowledged(result);
  await stream("agent-1", "The answer");
  expect(result.current.conversation.stream).toBe("The answer");
  const refreshed = deferred<any>();
  mocks.conversationSnapshot.mockImplementationOnce(() => refreshed.promise);
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "completed",
  });
  expect(result.current.conversation.active).toBeNull();
  expect(result.current.conversation.pendingUser).toBe("Question");
  expect(result.current.conversation.stream).toBe("The answer");
  await act(async () => {
    refreshed.resolve(recordedTurn(saved(), "The answer"));
  });
  await waitFor(() => expect(result.current.conversation.stream).toBe(""));
  expect(result.current.conversation.pendingUser).toBeNull();
  expect(result.current.transcript.renderedItems).toHaveLength(2);
  unmount();
});

it("separates assistant messages that stream within one turn", async () => {
  const { result, unmount } = await draftFixture({ activeBinding: binding });
  await sendAcknowledged(result);
  await stream("agent-1", "I'll open the do-file first.");
  await stream("agent-2", "## Results");
  expect(result.current.conversation.stream).toBe(
    "I'll open the do-file first.\n\n## Results",
  );
  unmount();
});

it("a stopped response keeps its prompt in the transcript instead of restoring it for a second send", async () => {
  const { result, saved, unmount } = await draftFixture({
    activeBinding: binding,
  });
  await sendAcknowledged(result);
  await stream("agent-1", "Partial");
  const stopped = recordedTurn(saved(), "Partial");
  mocks.conversationSnapshot.mockImplementation(async () => stopped);
  mocks.reconcileSession.mockClear();
  mocks.updateSession.mockClear();
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "interrupted",
  });
  expect(result.current.conversation.notice).toBe("Response stopped.");
  await waitFor(() => expect(result.current.conversation.stream).toBe(""));
  await waitFor(() =>
    expect(result.current.transcript.renderedItems).toHaveLength(2),
  );
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
  expect(mocks.reconcileSession).toHaveBeenCalledWith("A");
  expect(result.current.persistence.draft).toBe("");
  expect(result.current.feedback.error).toBeNull();
  expect(mocks.updateSession).not.toHaveBeenCalledWith(
    expect.objectContaining({ draft: "Question" }),
  );
  unmount();
});

it("keeps an acknowledged message out of the composer when the saved page no longer reaches it", async () => {
  const { result, saved, unmount } = await draftFixture({
    activeBinding: binding,
  });
  await sendAcknowledged(result);
  // A long turn: the newest page holds only its later items.
  const truncated = recordedTurn(saved(), "Partial");
  truncated.olderCursor = { createdAt: "earlier", id: "older-item" };
  truncated.items = truncated.items.filter(
    (item: any) => item.itemKind !== "userMessage",
  );
  truncated.turns[0].state = "failed";
  mocks.conversationSnapshot.mockImplementation(async () => truncated);
  mocks.updateSession.mockClear();
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "failed",
  });
  expect(result.current.feedback.error).toBe("The response failed.");
  await waitFor(() =>
    expect(result.current.conversation.pendingUser).toBeNull(),
  );
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
  expect(result.current.persistence.draft).toBe("");
  expect(result.current.feedback.error).toBe("The response failed.");
  expect(localStorage.getItem("pipeline.pendingDraft.A")).toBeNull();
  expect(mocks.updateSession).not.toHaveBeenCalledWith(
    expect.objectContaining({ draft: "Question" }),
  );
  unmount();
});

it("describes a closed connection in plain words", async () => {
  const { result, unmount } = await draftFixture({ activeBinding: binding });
  await sendAcknowledged(result);
  await emit({ epoch: 1, kind: "connectionClosed" });
  expect(result.current.feedback.error).toBe(
    "The connection to the assistant closed before the response finished.",
  );
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
  expect(result.current.persistence.draft).toBe("");
  unmount();
});

it("shows approvals and blocked sends for a response running in a conversation the user left", async () => {
  vi.clearAllMocks();
  mocks.listeners.length = 0;
  localStorage.clear();
  localStorage.setItem("pipeline.workspace.sessionId", "A");
  const make = (id: string, providerThreadId: string) => ({
    workspace: null,
    session: {
      id,
      title: `Conversation ${id}`,
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
    activeBinding: { id: `binding-${id}`, providerThreadId },
    turns: [],
    items: [],
  });
  const snaps: any = { A: make("A", "thread-A"), B: make("B", "thread-B") };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [] });
  mocks.pendingRequests.mockResolvedValue([]);
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.listSessions.mockResolvedValue({
    sessions: [snaps.A.session, snaps.B.session],
  });
  mocks.reconcileSession.mockResolvedValue(false);
  mocks.conversationSnapshot.mockImplementation(
    async (id: string) => snaps[id],
  );
  mocks.sessionSnapshot.mockImplementation(async (id: string) => snaps[id]);
  mocks.updateSession.mockImplementation(async (r: any) => ({
    record: { ...snaps[r.sessionId].session, ...r, revision: 2 },
    sequence: 2,
  }));
  const { result, unmount } = renderHook(() => useWorkspacePageController({}));
  await waitFor(() =>
    expect(result.current.conversation.snapshot?.session.id).toBe("A"),
  );
  await sendAcknowledged(result);
  await act(async () => await result.current.navigation.selectSession("B"));
  await waitFor(() =>
    expect(result.current.conversation.snapshot?.session.id).toBe("B"),
  );
  await waitFor(() =>
    expect(result.current.conversation.backgroundTurn).toMatchObject({
      sessionId: "A",
      title: "Conversation A",
      waiting: 0,
    }),
  );
  await emit({
    epoch: 1,
    kind: "serverRequest",
    requestId: 9,
    method: "item/commandExecution/requestApproval",
    params: { threadId: "thread-A", turnId: "turn-1", command: "make" },
  });
  expect(result.current.conversation.backgroundTurn?.waiting).toBe(1);
  expect(result.current.conversation.visibleRequests).toHaveLength(0);
  act(() => result.current.persistence.editDraft("A question for B"));
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(mocks.sendTurn).toHaveBeenCalledTimes(1);
  expect(result.current.feedback.error).toContain(
    "still running in another conversation",
  );
  expect(result.current.persistence.draft).toBe("A question for B");
  unmount();
});

it("gives a file-change approval the files from the live item it refers to", async () => {
  const { result, unmount } = await draftFixture({ activeBinding: binding });
  const item = {
    type: "fileChange",
    id: "change-1",
    status: "inProgress",
    changes: [{ path: "paper.tex", kind: { type: "update" }, diff: "-a\n+b" }],
  };
  await emit({
    epoch: 1,
    kind: "itemStarted",
    threadId: "thread-A",
    turnId: "turn-1",
    itemId: "change-1",
    itemKind: "fileChange",
    item,
  });
  await emit({
    epoch: 1,
    kind: "serverRequest",
    requestId: 3,
    method: "item/fileChange/requestApproval",
    params: { threadId: "thread-A", turnId: "turn-1", itemId: "change-1" },
  });
  const [request] = result.current.conversation.visibleRequests;
  expect(result.current.conversation.requestItem(request)).toEqual(item);
  unmount();
});
