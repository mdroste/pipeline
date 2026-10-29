import { renderHook, act, waitFor } from "@testing-library/react";
import { it, expect, vi } from "vitest";
import { useWorkspacePageController } from "./useWorkspacePageController";
const mocks = vi.hoisted(() => ({
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
it.each(
  ["failed", "completed", "connectionClosed"].flatMap((status) =>
    [false, true].map((beforeAck) => ({ status, beforeAck })),
  ),
)(
  "does not restore a failed turn from conversation A into newly selected conversation B ($status, beforeAck=$beforeAck)",
  async ({ status, beforeAck }) => {
    vi.clearAllMocks();
    mocks.listeners.length = 0;
    localStorage.clear();
    localStorage.setItem("pipeline.workspace.sessionId", "A");
    const make = (id: string, draft: string) => ({
      workspace: null,
      session: {
        id,
        title: id,
        workspaceId: null,
        paperId: null,
        presetId: null,
        overrides: {},
        draft,
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
    const snaps: any = { A: make("A", ""), B: make("B", "B important draft") };
    mocks.listWorkspaces.mockResolvedValue({ workspaces: [] });
    mocks.pendingRequests.mockResolvedValue([]);
    mocks.connectCodex.mockResolvedValue({});
    mocks.accountState.mockResolvedValue({ status: "signedOut" });
    mocks.listSessions.mockImplementation(async () => ({
      sessions: Object.values(snaps).map((s: any) => s.session),
    }));
    mocks.conversationSnapshot.mockImplementation(
      async (id: string) => snaps[id],
    );
    mocks.sessionSnapshot.mockImplementation(async (id: string) => snaps[id]);
    mocks.reconcileSession.mockResolvedValue(false);
    const acknowledgement = {
      epoch: 1,
      threadId: "thread-A",
      turnId: "turn-A",
    };
    let acknowledge!: (value: typeof acknowledgement) => void;
    const acknowledged = new Promise<typeof acknowledgement>((resolve) => {
      acknowledge = resolve;
    });
    mocks.sendTurn.mockImplementation(() =>
      beforeAck ? acknowledged : Promise.resolve(acknowledgement),
    );
    mocks.updateSession.mockImplementation(async (r: any) => {
      snaps[r.sessionId] = {
        ...snaps[r.sessionId],
        session: { ...snaps[r.sessionId].session, ...r, revision: 2 },
      };
      return { record: snaps[r.sessionId].session, sequence: 2 };
    });
    const { result } = renderHook(() => useWorkspacePageController({}));
    await waitFor(() =>
      expect(result.current.conversation.snapshot?.session.id).toBe("A"),
    );
    act(() => result.current.persistence.editDraft("A original question"));
    let sending!: Promise<void>;
    await act(async () => {
      sending = result.current.conversation.send();
      if (!beforeAck) await sending;
    });
    await act(async () => await result.current.navigation.selectSession("B"));
    await waitFor(() =>
      expect(result.current.conversation.snapshot?.session.id).toBe("B"),
    );
    await act(async () => {
      for (const cb of mocks.listeners)
        cb({
          payload: {
            epoch: 1,
            kind: "agentMessageDelta",
            threadId: "thread-A",
            turnId: "turn-A",
            delta: "Only A",
          },
        });
    });
    expect(result.current.conversation.stream).toBe("");
    expect(result.current.persistence.draft).toBe("B important draft");
    await act(async () => {
      for (const cb of mocks.listeners)
        cb({
          payload: {
            epoch: 1,
            kind:
              status === "connectionClosed"
                ? "connectionClosed"
                : "turnCompleted",
            threadId: "thread-A",
            turnId: "turn-A",
            status,
          },
        });
    });
    if (beforeAck)
      await act(async () => {
        acknowledge(acknowledgement);
        await sending;
      });
    await waitFor(() =>
      expect(result.current.conversation.pendingUser).toBe(null),
    );
    expect.soft(result.current.persistence.draft).toBe("B important draft");
    if (status !== "completed")
      await waitFor(() =>
        expect(snaps.A.session.draft).toBe("A original question"),
      );
    expect(mocks.updateSession).not.toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: "B", draft: "A original question" }),
    );
  },
);

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
it("preserves a newly saved draft when an older hydration returns, including the next save", async () => {
  const { result, saved, unmount } = await draftFixture();
  const old = saved();
  let resolve!: (v: any) => void;
  mocks.conversationSnapshot.mockImplementationOnce(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  let loading!: Promise<void>;
  act(() => {
    loading = result.current.conversation.hydrate();
  });
  act(() => result.current.persistence.editDraft("important fresh draft"));
  await act(async () => {
    await result.current.persistence.saveCurrentDraft();
  });
  expect(saved().session.draft).toBe("important fresh draft");
  expect(localStorage.getItem("pipeline.pendingDraft.A")).toBeNull();
  await act(async () => {
    resolve(old);
    await loading;
  });
  expect(result.current.persistence.draft).toBe("important fresh draft");
  expect(result.current.conversation.snapshot?.session.revision).toBe(2);
  await act(async () => {
    await result.current.persistence.saveCurrentDraft();
  });
  expect(saved().session.draft).toBe("important fresh draft");
  unmount();
});
it("ignores a superseded same-session hydration", async () => {
  const { result, saved, unmount } = await draftFixture();
  let first!: (v: any) => void;
  let second!: (v: any) => void;
  mocks.conversationSnapshot
    .mockImplementationOnce(
      () =>
        new Promise((r) => {
          first = r;
        }),
    )
    .mockImplementationOnce(
      () =>
        new Promise((r) => {
          second = r;
        }),
    );
  let a!: Promise<void>;
  let b!: Promise<void>;
  act(() => {
    a = result.current.conversation.hydrate();
    b = result.current.conversation.hydrate();
  });
  const newer = {
    ...saved(),
    session: { ...saved().session, title: "new title", revision: 3 },
    sequence: 3,
  };
  await act(async () => {
    second(newer);
    await b;
  });
  await act(async () => {
    first(saved());
    await a;
  });
  expect(result.current.conversation.snapshot?.session.title).toBe("new title");
  unmount();
});

it("clears a restored active turn when reconciliation confirms it ended", async () => {
  const { result, saved, unmount } = await draftFixture({
    activeBinding: { id: "binding", providerThreadId: "thread" },
    turns: [
      {
        id: "turn",
        bindingId: "binding",
        providerTurnId: "turn",
        terminalAt: null,
        state: "inProgress",
      },
    ],
  });
  await waitFor(() =>
    expect(result.current.conversation.active?.turnId).toBe("turn"),
  );
  mocks.conversationSnapshot.mockResolvedValueOnce({
    ...saved(),
    turns: [{ ...saved().turns[0], terminalAt: "now", state: "completed" }],
    sequence: 2,
  });
  await act(async () => {
    await result.current.conversation.hydrate();
  });
  expect(result.current.conversation.active).toBeNull();
  unmount();
});

it("never pairs an unfinished historical turn with a successor binding", async () => {
  const { result, unmount } = await draftFixture({
    activeBinding: { id: "new-binding", providerThreadId: "new-thread" },
    turns: [
      {
        id: "old",
        bindingId: "old-binding",
        providerTurnId: "old-turn",
        terminalAt: null,
        state: "inProgress",
      },
      {
        id: "new",
        bindingId: "new-binding",
        providerTurnId: "new-turn",
        terminalAt: "now",
        state: "completed",
      },
    ],
  });
  expect(result.current.conversation.active).toBeNull();
  unmount();
});

it("restores the composer text when a dispatched send is rejected", async () => {
  const { result, saved, unmount } = await draftFixture();
  mocks.sendTurn.mockImplementation(async (r: any) => {
    // The host records the text as the session draft before dispatch.
    mocks.updateSession.getMockImplementation()!({
      sessionId: r.sessionId,
      draft: r.text,
    });
    throw new Error("project has a local job holding its write resources");
  });
  act(() => result.current.persistence.editDraft("my long question"));
  await act(async () => {
    await result.current.conversation.send();
  });
  await new Promise((resolve) => setTimeout(resolve, 600));
  expect(result.current.persistence.draft).toBe("my long question");
  expect(result.current.conversation.pendingUser).toBeNull();
  expect(mocks.updateSession).not.toHaveBeenCalledWith(
    expect.objectContaining({ sessionId: "A", draft: "" }),
  );
  expect(saved().session.draft).toBe("my long question");
  unmount();
});

it("finishes an automation-driven turn on the viewed thread instead of leaving a phantom bubble", async () => {
  const { result, unmount } = await draftFixture({
    activeBinding: { id: "binding", providerThreadId: "thread-A" },
  });
  mocks.reconcileSession.mockClear();
  mocks.conversationSnapshot.mockClear();
  await act(async () => {
    for (const cb of mocks.listeners)
      cb({
        payload: {
          epoch: 1,
          kind: "turnStarted",
          threadId: "thread-A",
          turnId: "turn-X",
        },
      });
  });
  expect(result.current.conversation.active?.turnId).toBe("turn-X");
  await act(async () => {
    for (const cb of mocks.listeners)
      cb({
        payload: {
          epoch: 1,
          kind: "agentMessageDelta",
          threadId: "thread-A",
          turnId: "turn-X",
          delta: "hi",
        },
      });
    await new Promise((resolve) => setTimeout(resolve, 80));
  });
  expect(result.current.conversation.stream).toBe("hi");
  await act(async () => {
    for (const cb of mocks.listeners)
      cb({
        payload: {
          epoch: 1,
          kind: "turnCompleted",
          threadId: "thread-A",
          turnId: "turn-X",
          status: "completed",
        },
      });
  });
  await waitFor(() => expect(result.current.conversation.active).toBeNull());
  expect(result.current.conversation.stream).toBe("");
  expect(mocks.reconcileSession).toHaveBeenCalledWith("A");
  await waitFor(() => expect(mocks.conversationSnapshot).toHaveBeenCalled());
  unmount();
});

it("releases a turn restored by hydration when its connection closes", async () => {
  const { result, unmount } = await draftFixture({
    activeBinding: { id: "binding", providerThreadId: "thread-A" },
    turns: [
      {
        id: "turn",
        bindingId: "binding",
        providerTurnId: "turn-A",
        terminalAt: null,
        state: "inProgress",
      },
    ],
  });
  await waitFor(() =>
    expect(result.current.conversation.active?.turnId).toBe("turn-A"),
  );
  mocks.reconcileSession.mockClear();
  await act(async () => {
    for (const cb of mocks.listeners)
      cb({ payload: { epoch: 1, kind: "connectionClosed" } });
  });
  await waitFor(() => expect(result.current.conversation.active).toBeNull());
  expect(mocks.reconcileSession).toHaveBeenCalledWith("A");
  unmount();
});

it("serializes model selection and the current draft through the same revision lane", async () => {
  const { result, saved, unmount } = await draftFixture();
  const update = mocks.updateSession.getMockImplementation()!;
  let release!: () => void;
  const blocked = new Promise<void>((resolve) => {
    release = resolve;
  });
  mocks.updateSession.mockImplementationOnce(async (request: any) => {
    await blocked;
    return update(request);
  });
  act(() =>
    result.current.persistence.saveSelection("chosen-model", "high", "claude"),
  );
  await waitFor(() => expect(mocks.updateSession).toHaveBeenCalledTimes(1));
  act(() => result.current.persistence.editDraft("saved after selection"));
  let saving!: Promise<void>;
  let completed = false;
  await act(async () => {
    saving = result.current.persistence.saveCurrentDraft().then(() => {
      completed = true;
    });
  });
  expect(completed).toBe(false);
  expect(mocks.updateSession).toHaveBeenCalledTimes(1);
  await act(async () => {
    release();
    await saving;
  });
  expect(mocks.updateSession).toHaveBeenNthCalledWith(
    1,
    expect.objectContaining({
      expectedRevision: 1,
      overrides: { runtime: "claude", model: "chosen-model", effort: "high" },
    }),
  );
  expect(mocks.updateSession).toHaveBeenNthCalledWith(
    2,
    expect.objectContaining({
      expectedRevision: 2,
      draft: "saved after selection",
    }),
  );
  expect(saved().session.draft).toBe("saved after selection");
  expect(saved().session.overrides.model).toBe("chosen-model");
  unmount();
});

it("does not dispatch when the selection save barrier failed and restores the draft", async () => {
  const { result, unmount } = await draftFixture();
  mocks.sendTurn.mockReset();
  mocks.updateSession.mockRejectedValueOnce(
    new Error("selection revision conflict"),
  );
  act(() => result.current.persistence.saveSelection("chosen-model", "high"));
  await waitFor(() =>
    expect(result.current.feedback.error).toContain(
      "Model selection could not be saved",
    ),
  );
  act(() => result.current.persistence.editDraft("keep this unsent question"));
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(mocks.sendTurn).not.toHaveBeenCalled();
  expect(result.current.persistence.draft).toBe("keep this unsent question");
  expect(localStorage.getItem("pipeline.pendingDraft.A")).toBe(
    "keep this unsent question",
  );
  expect(result.current.conversation.pendingUser).toBeNull();
  expect(result.current.conversation.submitting).toBe(false);
  expect(result.current.feedback.error).toContain(
    "Message not sent because its settings or draft could not be saved",
  );
  unmount();
});

it("preserves a newer composer edit after an ambiguously dispatched send fails without resending", async () => {
  const { result, unmount } = await draftFixture();
  let reject!: (error: Error) => void;
  mocks.sendTurn.mockImplementationOnce(
    () =>
      new Promise((_resolve, fail) => {
        reject = fail;
      }),
  );
  act(() => result.current.persistence.editDraft("first question"));
  let sending!: Promise<void>;
  await act(async () => {
    sending = result.current.conversation.send();
  });
  await waitFor(() => expect(mocks.sendTurn).toHaveBeenCalledTimes(1));
  act(() =>
    result.current.persistence.editDraft("new question typed while waiting"),
  );
  await act(async () => {
    reject(new Error("acknowledgement lost"));
    await sending;
  });
  expect(mocks.sendTurn).toHaveBeenCalledTimes(1);
  expect(result.current.persistence.draft).toBe(
    "new question typed while waiting",
  );
  expect(localStorage.getItem("pipeline.pendingDraft.A")).toBe(
    "new question typed while waiting",
  );
  expect(result.current.conversation.pendingUser).toBeNull();
  expect(result.current.feedback.error).toContain(
    "will not be retried automatically",
  );
  unmount();
});
