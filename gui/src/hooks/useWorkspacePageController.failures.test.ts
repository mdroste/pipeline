import { renderHook, act, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
  activeTurn: vi.fn(),
  notify: vi.fn(),
  confirmDialog: vi.fn(),
  deleteSession: vi.fn(),
  moveSession: vi.fn(),
  resolveServerRequest: vi.fn(),
  listeners: [] as any[],
}));
vi.mock("../lib/workbench/workspaceClient", () => ({ workspaceClient: mocks }));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: mocks,
}));
vi.mock("../lib/workbench/runtimeClient", () => ({ runtimeClient: mocks }));
vi.mock("../components/DialogService", () => ({
  notify: mocks.notify,
  confirmDialog: mocks.confirmDialog,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: any) => {
    if (name === "workbench:event") mocks.listeners.push(cb);
    return () => {};
  }),
}));

const make = (id: string) => ({
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
  activeBinding: { id: `binding-${id}`, providerThreadId: `thread-${id}` },
  turns: [],
  items: [],
});
type Result = {
  current: ReturnType<typeof useWorkspacePageController>;
};
const emit = async (payload: Record<string, unknown>) => {
  await act(async () => {
    for (const cb of mocks.listeners) cb({ payload });
  });
};
const open = async (result: Result, id: string) => {
  await act(async () => await result.current.navigation.selectSession(id));
  await waitFor(() =>
    expect(result.current.conversation.snapshot?.session.id).toBe(id),
  );
};
const type = (result: Result, text: string) =>
  act(() => result.current.persistence.editDraft(text));

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listeners.length = 0;
  localStorage.clear();
  localStorage.setItem("pipeline.workspace.sessionId", "A");
  const snaps: Record<string, ReturnType<typeof make>> = {
    A: make("A"),
    B: make("B"),
  };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [] });
  mocks.pendingRequests.mockResolvedValue([]);
  mocks.connectCodex.mockResolvedValue({});
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.activeTurn.mockResolvedValue(null);
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
  mocks.sendTurn.mockResolvedValue({
    epoch: 1,
    threadId: "thread-A",
    turnId: "turn-1",
  });
});

async function mount() {
  const hook = renderHook(() => useWorkspacePageController({}));
  await waitFor(() =>
    expect(hook.result.current.conversation.snapshot?.session.id).toBe("A"),
  );
  return hook;
}

it("does not let one failed model save block Send afterwards", async () => {
  const { result, unmount } = await mount();
  mocks.updateSession.mockRejectedValueOnce(new Error("revision conflict"));
  act(() => result.current.persistence.saveSelection("chosen", "high"));
  await waitFor(() =>
    expect(result.current.feedback.error).toContain(
      "Model selection could not be saved",
    ),
  );
  await open(result, "B");
  type(result, "A question in another conversation");
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(mocks.sendTurn).toHaveBeenCalledWith(
    expect.objectContaining({ sessionId: "B" }),
  );
  expect(result.current.feedback.error).toBeNull();
  unmount();
});

it("does not let a failed draft autosave block Send", async () => {
  const { result, unmount } = await mount();
  mocks.updateSession.mockRejectedValueOnce(new Error("store busy"));
  type(result, "Question");
  await waitFor(() =>
    expect(result.current.feedback.error).toContain("Draft could not be saved"),
  );
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(mocks.sendTurn).toHaveBeenCalledTimes(1);
  unmount();
});

it("reports a refused send after the researcher left its conversation", async () => {
  const { result, unmount } = await mount();
  let refuse!: (cause: unknown) => void;
  mocks.sendTurn.mockImplementationOnce(
    () => new Promise((_resolve, reject) => (refuse = reject)),
  );
  type(result, "Question");
  let sending!: Promise<void>;
  await act(async () => {
    sending = result.current.conversation.send();
  });
  await waitFor(() => expect(refuse).toBeDefined());
  await open(result, "B");
  await act(async () => {
    refuse({ code: "turn_not_sent", message: "Sign in to ChatGPT again." });
    await sending;
  });

  // Not on the conversation it doesn't belong to, but announced with a way back.
  expect(result.current.feedback.error).toBeNull();
  expect(mocks.notify).toHaveBeenCalledWith(
    "Message not sent: Sign in to ChatGPT again.",
    "error",
    expect.objectContaining({ label: "Open conversation" }),
  );

  await open(result, "A");
  expect(result.current.feedback.error).toBe(
    "Message not sent: Sign in to ChatGPT again.",
  );
  expect(result.current.persistence.draft).toBe("Question");

  // Said once: it does not come back on a later visit.
  await open(result, "B");
  await open(result, "A");
  expect(result.current.feedback.error).toBeNull();
  unmount();
});

it("keeps an unconfirmed send as a draft after the researcher left its conversation", async () => {
  const { result, unmount } = await mount();
  let lose!: (cause: unknown) => void;
  mocks.sendTurn.mockImplementationOnce(
    () => new Promise((_resolve, reject) => (lose = reject)),
  );
  type(result, "Question");
  let sending!: Promise<void>;
  await act(async () => {
    sending = result.current.conversation.send();
  });
  await waitFor(() => expect(lose).toBeDefined());
  await open(result, "B");
  await act(async () => {
    lose(new Error("connection reset"));
    await sending;
  });
  expect(mocks.notify).toHaveBeenCalledTimes(1);
  await open(result, "A");
  expect(result.current.feedback.error).toContain(
    "could not confirm whether the message was received",
  );
  expect(result.current.persistence.draft).toBe("Question");
  unmount();
});

const send = async (result: Result) => {
  type(result, "Question");
  await act(async () => {
    await result.current.conversation.send();
  });
};

it("names the reason a turn failed", async () => {
  const { result, unmount } = await mount();
  await send(result);
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "failed",
    turn: {
      id: "turn-1",
      status: "failed",
      error: {
        message: "You have reached your usage limit",
        codexErrorInfo: "usageLimitExceeded",
      },
    },
  });
  await waitFor(() =>
    expect(result.current.feedback.error).toBe(
      "The response failed: You have reached your usage limit.",
    ),
  );
  unmount();
});

it("still says the response failed when no reason came with it", async () => {
  const { result, unmount } = await mount();
  await send(result);
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "failed",
    turn: { id: "turn-1", status: "failed", error: null },
  });
  expect(result.current.feedback.error).toBe("The response failed.");
  unmount();
});

it("redraws an approval when its file change arrives after it", async () => {
  const { result, unmount } = await mount();
  await send(result);
  await emit({
    epoch: 1,
    kind: "serverRequest",
    requestId: 3,
    method: "item/fileChange/requestApproval",
    params: { threadId: "thread-A", turnId: "turn-1", itemId: "change-1" },
  });
  const [request] = result.current.conversation.visibleRequests;
  const before = result.current.conversation.requestItem;
  expect(before(request)).toBeNull();
  const item = {
    type: "fileChange",
    id: "change-1",
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
  // A new lookup means the page drew again with the item.
  expect(result.current.conversation.requestItem).not.toBe(before);
  expect(before(request)).toBeNull();
  expect(result.current.conversation.requestItem(request)).toEqual(item);
  unmount();
});

const captureFailed =
  "The working copy could not be captured: a file is too large. Its files are retained; refresh before accepting changes.";

it("shows a warning on the conversation whose response raised it, until the next send", async () => {
  const { result, unmount } = await mount();
  await send(result);
  await emit({ epoch: 1, kind: "protocolWarning", message: captureFailed });
  await emit({ epoch: 1, kind: "protocolWarning", message: captureFailed });
  await emit({
    epoch: 1,
    kind: "protocolWarning",
    message: "Ignored response for unknown or expired request 9",
  });
  expect(result.current.conversation.warnings).toEqual([captureFailed]);
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "interrupted",
    turn: { id: "turn-1", status: "interrupted" },
  });
  await waitFor(() => expect(result.current.conversation.active).toBeNull());
  // The stopped-response notice must not take its place.
  expect(result.current.conversation.warnings).toEqual([captureFailed]);
  await send(result);
  expect(result.current.conversation.warnings).toEqual([]);
  unmount();
});

it("reports an accepted turn that fails after the researcher left", async () => {
  const { result, unmount } = await mount();
  await send(result);
  await open(result, "B");
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "failed",
    turn: {
      id: "turn-1",
      status: "failed",
      error: { message: "You have reached your usage limit" },
    },
  });
  const problem = "The response failed: You have reached your usage limit.";
  await waitFor(() =>
    expect(mocks.notify).toHaveBeenCalledWith(
      problem,
      "error",
      expect.objectContaining({ label: "Open conversation" }),
    ),
  );
  expect(result.current.feedback.error).toBeNull();
  await open(result, "A");
  expect(result.current.feedback.error).toBe(problem);
  unmount();
});

it("does not let a later warning replace a waiting failure", async () => {
  const { result, unmount } = await mount();
  await send(result);
  await open(result, "B");
  await emit({ epoch: 1, kind: "protocolWarning", message: captureFailed });
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "failed",
    turn: { id: "turn-1", status: "failed", error: { message: "It stopped" } },
  });
  const problem = "The response failed: It stopped.";
  await waitFor(() =>
    expect(mocks.notify).toHaveBeenCalledWith(
      problem,
      "error",
      expect.anything(),
    ),
  );
  await emit({ epoch: 1, kind: "protocolWarning", message: captureFailed });
  await open(result, "A");
  expect(result.current.feedback.error).toBe(problem);
  unmount();
});

it("carries a warning back to a conversation the researcher left", async () => {
  const { result, unmount } = await mount();
  await send(result);
  await open(result, "B");
  await emit({ epoch: 1, kind: "protocolWarning", message: captureFailed });
  expect(result.current.conversation.warnings).toEqual([]);
  expect(mocks.notify).toHaveBeenCalledWith(
    captureFailed,
    "error",
    expect.objectContaining({ label: "Open conversation" }),
  );
  await open(result, "A");
  expect(result.current.feedback.error).toBe(captureFailed);
  unmount();
});

it("announces a warning with no conversation of its own", async () => {
  const { result, unmount } = await mount();
  await emit({ epoch: 1, kind: "protocolWarning", message: captureFailed });
  expect(result.current.conversation.warnings).toEqual([]);
  expect(mocks.notify).toHaveBeenCalledWith(captureFailed, "info");
  unmount();
});

describe("changing another conversation", () => {
  // Open but unlisted, as an archived or older conversation is.
  const unlisted = async () => {
    const hook = await mount();
    const C = make("C");
    mocks.sessionSnapshot.mockImplementation(async (id: string) =>
      id === "C" ? C : make(id),
    );
    mocks.conversationSnapshot.mockImplementation(async (id: string) =>
      id === "C" ? C : make(id),
    );
    mocks.listSessions.mockResolvedValue({
      sessions: [make("A").session],
    });
    mocks.deleteSession.mockResolvedValue(undefined);
    mocks.moveSession.mockResolvedValue(undefined);
    mocks.confirmDialog.mockResolvedValue(true);
    await open(hook.result, "C");
    return hook;
  };
  const stayed = async (result: Result) => {
    await waitFor(() => expect(mocks.listSessions).toHaveBeenCalled());
    expect(result.current.conversation.snapshot?.session.id).toBe("C");
  };

  it("stays on an open conversation the list omits after an archive", async () => {
    const { result, unmount } = await unlisted();
    mocks.listSessions.mockClear();
    await act(async () => {
      await result.current.mutations.archiveSession(make("B").session as any);
    });
    await stayed(result);
    unmount();
  });

  it("stays on an open conversation the list omits after a delete", async () => {
    const { result, unmount } = await unlisted();
    mocks.listSessions.mockClear();
    await act(async () => {
      await result.current.mutations.deleteSession(make("B").session as any);
    });
    await stayed(result);
    unmount();
  });

  it("stays on an open conversation the list omits after a move", async () => {
    const { result, unmount } = await unlisted();
    mocks.listSessions.mockClear();
    await act(async () => {
      await result.current.mutations.moveSession(
        make("B").session as any,
        "project",
      );
    });
    await stayed(result);
    unmount();
  });

  it("still chooses another when the open conversation is the one deleted", async () => {
    const { result, unmount } = await unlisted();
    await act(async () => {
      await result.current.mutations.deleteSession(make("C").session as any);
    });
    await waitFor(() =>
      expect(result.current.conversation.snapshot?.session.id).toBe("A"),
    );
    unmount();
  });
});

it("keeps an autosave failure off the conversation opened after it", async () => {
  const { result, unmount } = await mount();
  let fail!: (cause: unknown) => void;
  mocks.updateSession.mockImplementationOnce(
    () => new Promise((_resolve, reject) => (fail = reject)),
  );
  type(result, "Half a thought");
  await waitFor(() => expect(fail).toBeDefined());
  mocks.deleteSession.mockResolvedValue(undefined);
  mocks.confirmDialog.mockResolvedValue(true);
  mocks.listSessions.mockResolvedValue({ sessions: [make("B").session] });
  await act(async () => {
    await result.current.mutations.deleteSession(make("A").session as any);
  });
  await waitFor(() =>
    expect(result.current.conversation.snapshot?.session.id).toBe("B"),
  );
  await act(async () => {
    fail({ code: "store_busy", message: "The store is busy." });
  });
  expect(result.current.feedback.error).toBeNull();
  unmount();
});

it("shows a failed approval decision on its own conversation", async () => {
  const { result, unmount } = await mount();
  let fail!: (cause: unknown) => void;
  mocks.resolveServerRequest.mockImplementationOnce(
    () => new Promise((_resolve, reject) => (fail = reject)),
  );
  const request = {
    epoch: 1,
    kind: "serverRequest",
    sessionId: "A",
    requestId: 7,
    method: "item/commandExecution/requestApproval",
    params: { threadId: "thread-A" },
  };
  let deciding!: Promise<void>;
  await act(async () => {
    deciding = result.current.conversation.resolve(request as any, {
      decision: "accept",
    });
  });
  await open(result, "B");
  await act(async () => {
    fail({ code: "connection_lost", message: "The connection was lost." });
    await deciding;
  });

  expect(result.current.feedback.error).toBeNull();
  expect(mocks.notify).toHaveBeenCalledWith(
    "The connection was lost.",
    "error",
    expect.objectContaining({ label: "Open conversation" }),
  );
  await open(result, "A");
  expect(result.current.feedback.error).toBe("The connection was lost.");
  unmount();
});

describe("a conversation that failed to load", () => {
  const failedLoad = async (props: Record<string, unknown> = {}) => {
    mocks.conversationSnapshot.mockRejectedValueOnce({
      code: "store_busy",
      message: "The store is busy.",
    });
    const hook = renderHook(
      (next: Record<string, unknown>) => useWorkspacePageController(next),
      { initialProps: props },
    );
    await waitFor(() =>
      expect(hook.result.current.feedback.error).toBe("The store is busy."),
    );
    expect(hook.result.current.conversation.snapshot).toBeNull();
    return hook;
  };
  const loaded = async (result: Result) => {
    await waitFor(() =>
      expect(result.current.conversation.snapshot?.session.id).toBe("A"),
    );
    expect(result.current.feedback.error).toBeNull();
  };

  it("loads again when its row is clicked", async () => {
    const { result, unmount } = await failedLoad();
    await act(async () => await result.current.navigation.selectSession("A"));
    await loaded(result);
    unmount();
  });

  it("loads again when a link opens it", async () => {
    const { result, rerender, unmount } = await failedLoad();
    rerender({
      entryTarget: { page: "workspace", sessionId: "A" },
      entryRequest: 1,
    });
    await loaded(result);
    unmount();
  });

  it("reads nothing again once it is on screen", async () => {
    const { result, unmount } = await mount();
    mocks.conversationSnapshot.mockClear();
    await act(async () => await result.current.navigation.selectSession("A"));
    expect(mocks.conversationSnapshot).not.toHaveBeenCalled();
    unmount();
  });
});
