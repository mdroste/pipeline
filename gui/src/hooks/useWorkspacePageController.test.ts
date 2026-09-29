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
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
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
    await waitFor(() => expect(result.current.snapshot?.session.id).toBe("A"));
    act(() => result.current.editDraft("A original question"));
    let sending!: Promise<void>;
    await act(async () => {
      sending = result.current.send();
      if (!beforeAck) await sending;
    });
    await act(async () => await result.current.selectSession("B"));
    await waitFor(() => expect(result.current.snapshot?.session.id).toBe("B"));
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
    expect(result.current.stream).toBe("");
    expect(result.current.draft).toBe("B important draft");
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
    await waitFor(() => expect(result.current.pendingUser).toBe(null));
    expect.soft(result.current.draft).toBe("B important draft");
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
    expect(hook.result.current.snapshot?.session.id).toBe("A"),
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
    loading = result.current.hydrate();
  });
  act(() => result.current.editDraft("important fresh draft"));
  await act(async () => {
    await result.current.saveCurrentDraft();
  });
  expect(saved().session.draft).toBe("important fresh draft");
  expect(localStorage.getItem("pipeline.pendingDraft.A")).toBeNull();
  await act(async () => {
    resolve(old);
    await loading;
  });
  expect(result.current.draft).toBe("important fresh draft");
  expect(result.current.snapshot?.session.revision).toBe(2);
  await act(async () => {
    await result.current.saveCurrentDraft();
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
    a = result.current.hydrate();
    b = result.current.hydrate();
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
  expect(result.current.snapshot?.session.title).toBe("new title");
  unmount();
});

it("clears a restored active turn when reconciliation confirms it ended", async () => {
  const { result, saved, unmount } = await draftFixture({
    activeBinding: { providerThreadId: "thread" },
    turns: [
      {
        id: "turn",
        providerTurnId: "turn",
        terminalAt: null,
        state: "inProgress",
      },
    ],
  });
  await waitFor(() => expect(result.current.active?.turnId).toBe("turn"));
  mocks.conversationSnapshot.mockResolvedValueOnce({
    ...saved(),
    turns: [{ ...saved().turns[0], terminalAt: "now", state: "completed" }],
    sequence: 2,
  });
  await act(async () => {
    await result.current.hydrate();
  });
  expect(result.current.active).toBeNull();
  unmount();
});
