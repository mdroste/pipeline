import { renderHook, act, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useWorkspacePageController } from "./useWorkspacePageController";
import { startupRoute } from "../lib/appPreferences";
import { router } from "../lib/router";
import type { ActiveTurn, Workspace } from "../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({
  activeTurn: vi.fn(),
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
  interruptTurn: vi.fn(),
  listeners: [] as any[],
}));
vi.mock("../lib/workbench/workspaceClient", () => ({ workspaceClient: mocks }));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: mocks,
}));
vi.mock("../lib/workbench/runtimeClient", () => ({ runtimeClient: mocks }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: any) => {
    if (name === "workbench:event") mocks.listeners.push(cb);
    return () => {};
  }),
}));

const emit = async (payload: Record<string, unknown>) => {
  await act(async () => {
    for (const cb of mocks.listeners) cb({ payload });
  });
};
const settle = () =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });

let saved: Record<string, any>;
const conversation = (id: string, draft = "", binding = true) => ({
  workspace: null,
  session: {
    id,
    title: `Conversation ${id}`,
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
  activeBinding: binding
    ? { id: `binding-${id}`, providerThreadId: `thread-${id}` }
    : null,
  turns: [],
  items: [],
});
const hostTurn = (overrides: Partial<ActiveTurn> = {}): ActiveTurn => ({
  sessionId: "A",
  title: "Conversation A",
  threadId: "thread-A",
  turnId: "turn-1",
  epoch: 1,
  setup: false,
  runtime: "codex",
  automated: false,
  composer: true,
  ...overrides,
});
const approval = {
  epoch: 1,
  kind: "serverRequest",
  requestId: 9,
  method: "item/commandExecution/requestApproval",
  params: { threadId: "thread-A", turnId: "turn-1", command: "make" },
  sessionId: "A",
};

async function open(id: string) {
  mocks.listeners.length = 0;
  localStorage.setItem("pipeline.workspace.sessionId", id);
  const hook = renderHook(() => useWorkspacePageController({}));
  await waitFor(() =>
    expect(hook.result.current.conversation.snapshot?.session.id).toBe(id),
  );
  await settle();
  return hook;
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.listeners.length = 0;
  localStorage.clear();
  saved = { A: conversation("A"), B: conversation("B") };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [] });
  mocks.pendingRequests.mockResolvedValue([]);
  mocks.connectCodex.mockResolvedValue({});
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.activeTurn.mockResolvedValue(null);
  mocks.listSessions.mockImplementation(async () => ({
    sessions: Object.values(saved).map((item) => item.session),
  }));
  mocks.reconcileSession.mockResolvedValue(false);
  mocks.conversationSnapshot.mockImplementation(
    async (id: string) => saved[id],
  );
  mocks.sessionSnapshot.mockImplementation(async (id: string) => saved[id]);
  mocks.updateSession.mockImplementation(async (request: any) => {
    const current = saved[request.sessionId];
    saved[request.sessionId] = {
      ...current,
      session: {
        ...current.session,
        ...request,
        revision: current.session.revision + 1,
      },
      sequence: current.sequence + 1,
    };
    return {
      record: saved[request.sessionId].session,
      sequence: saved[request.sessionId].sequence,
    };
  });
  mocks.interruptTurn.mockResolvedValue(undefined);
});

it("shows a waiting approval from another conversation after the page remounts", async () => {
  mocks.sendTurn.mockResolvedValueOnce({
    epoch: 1,
    threadId: "thread-A",
    turnId: "turn-1",
  });
  const first = await open("A");
  act(() => first.result.current.persistence.editDraft("Question"));
  await act(async () => {
    await first.result.current.conversation.send();
  });
  first.unmount();

  mocks.activeTurn.mockResolvedValue(hostTurn());
  mocks.pendingRequests.mockResolvedValue([approval]);
  const { result, unmount } = await open("B");
  expect(result.current.conversation.backgroundTurn).toEqual({
    sessionId: "A",
    title: "Conversation A",
    waiting: 1,
    automated: false,
  });
  expect(result.current.conversation.visibleRequests).toHaveLength(0);
  unmount();
});

it("counts an approval that arrives while the running conversation is out of view", async () => {
  mocks.activeTurn.mockResolvedValue(hostTurn());
  const { result, unmount } = await open("B");
  expect(result.current.conversation.backgroundTurn?.waiting).toBe(0);
  await emit(approval);
  expect(result.current.conversation.backgroundTurn?.waiting).toBe(1);
  await emit({ epoch: 1, kind: "serverRequestResolved", requestId: 9 });
  expect(result.current.conversation.backgroundTurn?.waiting).toBe(0);
  unmount();
});

it("follows the host when a running turn ends in the background", async () => {
  mocks.activeTurn.mockResolvedValue(hostTurn());
  const { result, unmount } = await open("B");
  expect(result.current.conversation.backgroundTurn?.sessionId).toBe("A");
  mocks.activeTurn.mockResolvedValue(null);
  await emit({ epoch: 0, kind: "activeTurnChanged" });
  await waitFor(() =>
    expect(result.current.conversation.backgroundTurn).toBeNull(),
  );
  unmount();
});

it("reports a busy host at once and keeps the message for a later send", async () => {
  mocks.activeTurn.mockResolvedValue(hostTurn());
  mocks.sendTurn.mockRejectedValueOnce({
    code: "turn_busy",
    message: "Another response is still running",
    retryable: true,
  });
  const { result, unmount } = await open("B");
  act(() => result.current.persistence.editDraft("A question for B"));
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(mocks.sendTurn).toHaveBeenCalledTimes(1);
  expect(result.current.feedback.error).toBe(
    "Message not sent: a response is still running in “Conversation A”. Send it when that response finishes, or open that conversation to stop it.",
  );
  expect(result.current.persistence.draft).toBe("A question for B");
  expect(result.current.conversation.pendingUser).toBeNull();
  expect(result.current.conversation.submitting).toBe(false);
  expect(result.current.conversation.backgroundTurn?.sessionId).toBe("A");

  // The refused text is an ordinary draft: one later send, one delivery.
  mocks.activeTurn.mockResolvedValue(null);
  await emit({ epoch: 0, kind: "activeTurnChanged" });
  await waitFor(() =>
    expect(result.current.conversation.backgroundTurn).toBeNull(),
  );
  mocks.sendTurn.mockResolvedValueOnce({
    epoch: 1,
    threadId: "thread-B",
    turnId: "turn-2",
  });
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(mocks.sendTurn).toHaveBeenCalledTimes(2);
  expect(mocks.sendTurn.mock.lastCall?.[0]).toMatchObject({
    sessionId: "B",
    text: "A question for B",
  });
  expect(result.current.persistence.draft).toBe("");
  expect(localStorage.getItem("pipeline.pendingDraft.B")).toBeNull();
  unmount();
});

it("names the automation that refused a send and points at Automations", async () => {
  mocks.activeTurn.mockResolvedValue(
    hostTurn({ automated: true, composer: false }),
  );
  mocks.sendTurn.mockRejectedValueOnce({ code: "turn_busy", message: "Busy" });
  const { result, unmount } = await open("B");
  expect(result.current.conversation.backgroundTurn?.automated).toBe(true);
  act(() => result.current.persistence.editDraft("Later"));
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(result.current.feedback.error).toContain(
    "an automation is running in “Conversation A”",
  );
  expect(result.current.feedback.error).toContain("open Automations");
  expect(result.current.persistence.draft).toBe("Later");
  unmount();
});

it("explains a busy host when nothing is left running to name", async () => {
  mocks.sendTurn.mockRejectedValueOnce({ code: "turn_busy", message: "Busy" });
  const { result, unmount } = await open("B");
  act(() => result.current.persistence.editDraft("Again"));
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(result.current.feedback.error).toBe(
    "Message not sent: Pipeline was busy for a moment. Send it again.",
  );
  expect(result.current.persistence.draft).toBe("Again");
  unmount();
});

const unacknowledged = () =>
  hostTurn({ threadId: null, turnId: null, epoch: null, setup: true });

it("keeps an unacknowledged send locked after a remount with no second copy to send", async () => {
  saved.A = conversation("A", "Question");
  mocks.activeTurn.mockResolvedValue(unacknowledged());
  const { result, unmount } = await open("A");
  expect(result.current.conversation.submitting).toBe(true);
  expect(result.current.conversation.pendingUser).toBe("Question");
  expect(result.current.persistence.draft).toBe("");
  await act(async () => {
    await result.current.conversation.send();
  });
  expect(mocks.sendTurn).not.toHaveBeenCalled();

  // Waiting past the autosave delay must not clear the saved copy either.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 400));
  });
  expect(mocks.updateSession).not.toHaveBeenCalled();
  unmount();
  expect(mocks.updateSession).not.toHaveBeenCalled();
});

it("returns the message when an unacknowledged send fails definitely", async () => {
  saved.A = conversation("A", "Question");
  mocks.activeTurn.mockResolvedValue(unacknowledged());
  const { result, unmount } = await open("A");
  mocks.activeTurn.mockResolvedValue(null);
  await emit({
    epoch: 0,
    kind: "activeTurnChanged",
    sessionId: "A",
    sendError: { code: "turn_not_sent", message: "Model unavailable" },
  });
  await waitFor(() =>
    expect(result.current.persistence.draft).toBe("Question"),
  );
  expect(result.current.conversation.submitting).toBe(false);
  expect(result.current.conversation.pendingUser).toBeNull();
  await waitFor(() =>
    expect(result.current.feedback.error).toBe(
      "Message not sent: Model unavailable",
    ),
  );
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 400));
  });
  expect(saved.A.session.draft).toBe("Question");
  unmount();
});

it("warns instead of calling an uncertain send unsent after a remount", async () => {
  saved.A = conversation("A", "Question");
  mocks.activeTurn.mockResolvedValue(unacknowledged());
  const { result, unmount } = await open("A");
  mocks.activeTurn.mockResolvedValue(null);
  await emit({
    epoch: 0,
    kind: "activeTurnChanged",
    sessionId: "A",
    sendError: { code: "transport_lost", message: "Connection lost" },
  });
  await waitFor(() =>
    expect(result.current.feedback.error).toContain(
      "could not confirm whether the message was received",
    ),
  );
  expect(result.current.persistence.draft).toBe("Question");
  expect(mocks.sendTurn).not.toHaveBeenCalled();
  unmount();
});

it("takes over an unacknowledged send once it is accepted, with Stop on the running turn", async () => {
  saved.A = conversation("A", "Question");
  mocks.activeTurn.mockResolvedValue(unacknowledged());
  const { result, unmount } = await open("A");
  saved.A = conversation("A", "");
  mocks.activeTurn.mockResolvedValue(hostTurn());
  await emit({ epoch: 0, kind: "activeTurnChanged" });
  await waitFor(() =>
    expect(result.current.conversation.active?.turnId).toBe("turn-1"),
  );
  expect(result.current.conversation.submitting).toBe(false);
  expect(result.current.persistence.draft).toBe("");
  await act(async () => {
    await result.current.conversation.stopCurrent();
  });
  expect(mocks.interruptTurn).toHaveBeenCalledWith("thread-A", "turn-1");

  mocks.activeTurn.mockResolvedValue(null);
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "failed",
  });
  await waitFor(() => expect(result.current.conversation.active).toBeNull());
  await settle();
  expect(result.current.persistence.draft).toBe("");
  unmount();
});

it("does not restore a message whose turn finished while setup was ending", async () => {
  saved.A = conversation("A", "Question");
  mocks.activeTurn.mockResolvedValue(unacknowledged());
  const { result, unmount } = await open("A");
  // The host cleared the draft on acknowledgement, before it announced the end.
  saved.A = conversation("A", "");
  mocks.activeTurn.mockResolvedValue(null);
  await emit({ epoch: 0, kind: "activeTurnChanged" });
  await waitFor(() =>
    expect(result.current.conversation.submitting).toBe(false),
  );
  expect(result.current.persistence.draft).toBe("");
  expect(result.current.feedback.error).toBeNull();
  unmount();
});

it("reaches Stop from the owning conversation when only the host knows the turn", async () => {
  mocks.activeTurn.mockResolvedValue(hostTurn({ setup: true }));
  const { result, unmount } = await open("A");
  expect(result.current.conversation.active).toEqual({
    threadId: "thread-A",
    turnId: "turn-1",
  });
  await act(async () => {
    await result.current.conversation.stopCurrent();
  });
  expect(mocks.interruptTurn).toHaveBeenCalledWith("thread-A", "turn-1");
  unmount();
});

it("leaves the composer alone for a follow-up or automation preparing in this conversation", async () => {
  saved.A = conversation("A", "My unsent note");
  mocks.activeTurn.mockResolvedValue(
    hostTurn({
      threadId: null,
      turnId: null,
      epoch: null,
      setup: true,
      automated: true,
      composer: false,
    }),
  );
  const automation = await open("A");
  const { conversation: view, persistence } = automation.result.current;
  expect(view.submitting).toBe(false);
  expect(view.pendingUser).toBeNull();
  expect(view.active).toBeNull();
  expect(view.backgroundTurn).toMatchObject({
    sessionId: "A",
    automated: true,
  });
  expect(persistence.draft).toBe("My unsent note");
  automation.unmount();

  mocks.activeTurn.mockResolvedValue(
    hostTurn({ threadId: null, turnId: null, setup: true, composer: false }),
  );
  const followUp = await open("A");
  expect(followUp.result.current.conversation.submitting).toBe(false);
  expect(followUp.result.current.persistence.draft).toBe("My unsent note");
  expect(followUp.result.current.conversation.backgroundTurn).toBeNull();
  followUp.unmount();
});

it("does not add Stop for an automation's running turn the transcript has not shown", async () => {
  mocks.activeTurn.mockResolvedValue(
    hostTurn({ automated: true, composer: false }),
  );
  const { result, unmount } = await open("A");
  expect(result.current.conversation.active).toBeNull();
  expect(result.current.conversation.backgroundTurn).toMatchObject({
    sessionId: "A",
    automated: true,
    waiting: 0,
  });
  unmount();
});

const delta = (text: string, itemId = "answer") => ({
  epoch: 1,
  kind: "agentMessageDelta",
  threadId: "thread-A",
  turnId: "turn-1",
  itemId,
  delta: text,
});
/** A running turn in conversation A whose prompt the host has saved. */
const savePrompt = (extra: Record<string, unknown>[] = []) => {
  saved.A = {
    ...saved.A,
    sequence: saved.A.sequence + 1,
    turns: [
      {
        id: "turn-record",
        bindingId: "binding-A",
        clientSubmissionId: "submission",
        providerTurnId: "turn-1",
        state: "inProgress",
        error: null,
        createdAt: "now",
        updatedAt: "now",
        terminalAt: null,
      },
    ],
    items: [
      {
        id: "item-prompt",
        turnId: "turn-record",
        providerItemId: "prompt",
        itemKind: "userMessage",
        payload: { text: "Question" },
        isFinal: true,
        createdAt: "now",
        updatedAt: "now",
      },
      ...extra,
    ],
  };
};
async function sendInA() {
  mocks.sendTurn.mockResolvedValueOnce({
    epoch: 1,
    threadId: "thread-A",
    turnId: "turn-1",
  });
  const hook = await open("A");
  act(() => hook.result.current.persistence.editDraft("Question"));
  await act(async () => {
    await hook.result.current.conversation.send();
  });
  mocks.activeTurn.mockResolvedValue(hostTurn());
  return hook;
}
const showing = (hook: Awaited<ReturnType<typeof open>>, id: string) =>
  waitFor(() =>
    expect(hook.result.current.conversation.snapshot?.session.id).toBe(id),
  );

it("clears a running response from the page when an empty project opens", async () => {
  mocks.listSessions.mockImplementation(async (workspace: string | null) => ({
    sessions: workspace ? [] : [saved.A.session, saved.B.session],
  }));
  const hook = await sendInA();
  await emit(delta("Partial answer"));
  const { conversation } = hook.result.current;
  await waitFor(() =>
    expect(conversation.streamStore.get()).toBe("Partial answer"),
  );
  await act(async () => {
    await hook.result.current.navigation.projectCreated({
      id: "empty",
      name: "Empty",
    } as Workspace);
  });
  await settle();
  const view = hook.result.current.conversation;
  expect(hook.result.current.navigation.sessionId).toBeNull();
  expect(view.pendingUser).toBeNull();
  expect(view.active).toBeNull();
  expect(view.submitting).toBe(false);
  expect(view.streamStore.get()).toBe("");
  expect(view.backgroundTurn?.sessionId).toBe("A");
  hook.unmount();
});

it("shows one copy of the prompt and the whole reply on returning to a running conversation", async () => {
  const hook = await sendInA();
  await emit(delta("Said while here. "));
  await waitFor(() =>
    expect(hook.result.current.conversation.streamStore.get()).toBe(
      "Said while here. ",
    ),
  );
  await act(async () => {
    await hook.result.current.navigation.selectSession("B");
  });
  await showing(hook, "B");
  await settle();
  expect(hook.result.current.conversation.streamStore.get()).toBe("");
  await emit(delta("Said while away. "));

  savePrompt();
  await act(async () => {
    await hook.result.current.navigation.selectSession("A");
  });
  await showing(hook, "A");
  await settle();
  const view = hook.result.current.conversation;
  expect(view.pendingUser).toBeNull();
  expect(view.active).toEqual({ threadId: "thread-A", turnId: "turn-1" });
  expect(view.streamStore.get()).toBe("Said while here. Said while away. ");

  await emit(delta("Said after returning."));
  await waitFor(() =>
    expect(hook.result.current.conversation.streamStore.get()).toBe(
      "Said while here. Said while away. Said after returning.",
    ),
  );
  hook.unmount();
});

it("keeps the prompt on screen until the transcript holds this turn's copy", async () => {
  const hook = await sendInA();
  await act(async () => {
    await hook.result.current.navigation.selectSession("B");
  });
  await showing(hook, "B");
  await act(async () => {
    await hook.result.current.navigation.selectSession("A");
  });
  await showing(hook, "A");
  await settle();
  expect(hook.result.current.conversation.pendingUser).toBe("Question");
  hook.unmount();
});

it("does not repeat a finished message of a running turn in the live reply", async () => {
  const hook = await sendInA();
  await emit(delta("First I will look.", "preamble"));
  await act(async () => {
    await hook.result.current.navigation.selectSession("B");
  });
  await showing(hook, "B");
  await emit(delta("Here is the answer", "answer"));
  savePrompt([
    {
      id: "item-preamble",
      turnId: "turn-record",
      providerItemId: "preamble",
      itemKind: "agentMessage",
      payload: { text: "First I will look." },
      isFinal: true,
      createdAt: "now",
      updatedAt: "now",
    },
  ]);
  await act(async () => {
    await hook.result.current.navigation.selectSession("A");
  });
  await showing(hook, "A");
  await settle();
  expect(hook.result.current.conversation.streamStore.get()).toBe(
    "Here is the answer",
  );
  hook.unmount();
});

it("forgets a finished turn's text, so a stale reply never returns", async () => {
  const hook = await sendInA();
  await emit(delta("Finished reply"));
  await act(async () => {
    await hook.result.current.navigation.selectSession("B");
  });
  await showing(hook, "B");
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "completed",
  });
  await settle();
  savePrompt();
  await act(async () => {
    await hook.result.current.navigation.selectSession("A");
  });
  await showing(hook, "A");
  await settle();
  expect(hook.result.current.conversation.streamStore.get()).toBe("");
  hook.unmount();
});

describe("the saved route", () => {
  const initial = router.getSnapshot().route;
  afterEach(() => router.init(initial));
  const openRoute = async (
    target: Extract<Parameters<typeof router.apply>[0], { page: "workspace" }>,
  ) => {
    router.apply(target);
    const hook = renderHook(() =>
      useWorkspacePageController({ entryTarget: target }),
    );
    await settle();
    return hook;
  };

  it("follows the project, conversation and destination chosen inside the page", async () => {
    saved.A.session.workspaceId = "P";
    saved.B.session.workspaceId = "Q";
    mocks.listSessions.mockImplementation(async (workspace: string | null) => ({
      sessions: workspace === "Q" ? [saved.B.session] : [saved.A.session],
    }));
    const { result, unmount } = await openRoute({
      page: "workspace",
      projectId: "P",
      surface: "project",
    });
    await act(async () => {
      await result.current.navigation.selectProject("Q");
    });
    await settle();
    expect(startupRoute(localStorage)).toMatchObject({
      page: "workspace",
      projectId: "Q",
      sessionId: "B",
      surface: "project",
    });
    act(() => result.current.navigation.navigateProject("writing"));
    expect(startupRoute(localStorage)).toMatchObject({
      projectId: "Q",
      destination: "writing",
    });
    unmount();
  });

  it("keeps the routed conversation until its entry has been applied", async () => {
    let release!: (value: unknown) => void;
    mocks.sessionSnapshot.mockImplementationOnce(
      () => new Promise((resolve) => (release = resolve)),
    );
    const { result, unmount } = await openRoute({
      page: "workspace",
      sessionId: "B",
      surface: "chat",
    });
    expect(startupRoute(localStorage)).toMatchObject({ sessionId: "B" });
    await act(async () => release(saved.B));
    await settle();
    expect(result.current.navigation.sessionId).toBe("B");
    expect(startupRoute(localStorage)).toMatchObject({ sessionId: "B" });
    unmount();
  });

  it("does not let a page that has been left replace the saved route", async () => {
    const { result, unmount } = await openRoute({
      page: "workspace",
      surface: "chat",
    });
    await act(async () => {
      await result.current.navigation.selectSession("A");
    });
    router.apply({ page: "tasks" });
    await act(async () => {
      await result.current.navigation.selectSession("B");
    });
    await settle();
    expect(startupRoute(localStorage)).toEqual({ page: "tasks" });
    unmount();
  });
});

it("still leaves a conversation after a model save fails, keeping Send's barrier", async () => {
  const { result, unmount } = await open("A");
  mocks.updateSession.mockRejectedValueOnce(new Error("revision conflict"));
  act(() => {
    result.current.conversation.setModel("chosen-model");
    result.current.conversation.setEffort("high");
    result.current.persistence.saveSelection("chosen-model", "high");
  });
  await waitFor(() =>
    expect(result.current.feedback.error).toContain(
      "Model selection could not be saved",
    ),
  );
  expect(result.current.conversation.model).toBe("");
  expect(result.current.conversation.effort).toBe("");
  await expect(
    result.current.persistence.saveCurrentDraft(),
  ).resolves.toBeUndefined();
  await act(async () => {
    await result.current.navigation.selectSession("B");
  });
  expect(result.current.navigation.sessionId).toBe("B");
  unmount();
});

it("does not undo a newer model choice when an earlier save fails", async () => {
  const { result, unmount } = await open("A");
  mocks.updateSession.mockRejectedValueOnce(new Error("revision conflict"));
  act(() => {
    result.current.conversation.setModel("first");
    result.current.persistence.saveSelection("first", "");
    result.current.conversation.setModel("second");
    result.current.persistence.saveSelection("second", "");
  });
  await waitFor(() => expect(saved.A.session.overrides.model).toBe("second"));
  await settle();
  expect(result.current.conversation.model).toBe("second");
  unmount();
});

describe("entering a conversation by route", () => {
  const entry = { page: "workspace", sessionId: "A" } as const;

  it("fills the list when the conversation loads before the list", async () => {
    let finishList!: () => void;
    const listed = new Promise<void>((resolve) => (finishList = resolve));
    const list = { sessions: [saved.A.session, saved.B.session] };
    mocks.listSessions.mockImplementation(() => listed.then(() => list));
    const { result, unmount } = renderHook(() =>
      useWorkspacePageController({ entryTarget: entry }),
    );
    await waitFor(() =>
      expect(result.current.conversation.snapshot?.session.id).toBe("A"),
    );
    expect(mocks.sessionSnapshot).toHaveBeenCalledWith("A");
    await act(async () => finishList());
    await waitFor(() =>
      expect(result.current.navigation.sessions).toHaveLength(2),
    );
    unmount();
  });

  it("keeps a conversation the list leaves out", async () => {
    mocks.listSessions.mockImplementation(async () => ({
      sessions: [saved.B.session],
    }));
    const { result, unmount } = renderHook(() =>
      useWorkspacePageController({ entryTarget: entry }),
    );
    await settle();
    await waitFor(() =>
      expect(result.current.navigation.sessions).toEqual([saved.B.session]),
    );
    expect(result.current.navigation.sessionId).toBe("A");
    unmount();
  });
});

describe("a conversation's load errors", () => {
  it("keeps a failed refresh on screen once the saved transcript appears", async () => {
    mocks.reconcileSession.mockRejectedValue(new Error("offline"));
    const { result, unmount } = await open("A");
    expect(result.current.feedback.error).toContain(
      "Could not refresh the conversation",
    );
    unmount();
  });

  it("keeps a failed list load on screen once the conversation appears", async () => {
    mocks.listSessions.mockRejectedValue(new Error("offline"));
    const { result, unmount } = await open("A");
    expect(result.current.feedback.error).toContain(
      "Could not load the conversation list",
    );
    unmount();
  });

  it("leaves an error behind with the conversation it belonged to", async () => {
    mocks.reconcileSession.mockRejectedValueOnce(new Error("offline"));
    const { result, unmount } = await open("A");
    expect(result.current.feedback.error).not.toBeNull();
    await act(async () => {
      await result.current.navigation.selectSession("B");
    });
    await settle();
    expect(result.current.conversation.snapshot?.session.id).toBe("B");
    expect(result.current.feedback.error).toBeNull();
    unmount();
  });
});

it("loads a conversation opened while the one left finishes", async () => {
  const hook = await sendInA();
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  mocks.conversationSnapshot.mockImplementation(async (id: string) => {
    if (id === "B") await held;
    return saved[id];
  });
  await act(async () => {
    await hook.result.current.navigation.selectSession("B");
  });
  await waitFor(() =>
    expect(mocks.conversationSnapshot).toHaveBeenCalledWith("B"),
  );
  await emit({
    epoch: 1,
    kind: "turnCompleted",
    threadId: "thread-A",
    turnId: "turn-1",
    status: "completed",
  });
  await act(async () => release());
  await showing(hook, "B");
  hook.unmount();
});

describe("a send reopened near the end of its setup", () => {
  it("does not offer a message whose turn was just acknowledged", async () => {
    saved.A = conversation("A", "Question");
    mocks.activeTurn.mockResolvedValue(hostTurn({ setup: true }));
    const { result, unmount } = await open("A");
    expect(result.current.persistence.draft).toBe("");
    expect(result.current.conversation.active?.turnId).toBe("turn-1");
    unmount();
  });

  it("does not take a message for a draft when setup ended during the read", async () => {
    saved.A = conversation("A", "Question");
    mocks.activeTurn.mockResolvedValue(hostTurn());
    const { result, unmount } = await open("A");
    expect(result.current.persistence.draft).toBe("");
    unmount();
  });

  it("still shows a real draft beside a follow-up running here", async () => {
    saved.A = conversation("A", "My next question");
    mocks.activeTurn.mockResolvedValue(hostTurn({ composer: false }));
    const { result, unmount } = await open("A");
    expect(result.current.persistence.draft).toBe("My next question");
    unmount();
  });

  it("stays locked when the unsent message can't be read back", async () => {
    saved.A = conversation("A", "Question");
    mocks.activeTurn.mockResolvedValue(unacknowledged());
    const { result, unmount } = await open("A");
    mocks.activeTurn.mockResolvedValue(null);
    mocks.conversationSnapshot.mockRejectedValue(new Error("offline"));
    await emit({ epoch: 0, kind: "activeTurnChanged" });
    await waitFor(() =>
      expect(result.current.feedback.error).toContain("stays locked"),
    );
    expect(result.current.conversation.submitting).toBe(true);

    // Neither autosave nor leaving may write the empty composer over the text.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 400));
    });
    mocks.conversationSnapshot.mockImplementation(
      async (id: string) => saved[id],
    );
    await act(async () => {
      await result.current.navigation.selectSession("B");
    });
    await settle();
    expect(saved.A.session.draft).toBe("Question");
    await act(async () => {
      await result.current.navigation.selectSession("A");
    });
    await waitFor(() =>
      expect(result.current.persistence.draft).toBe("Question"),
    );
    expect(result.current.conversation.submitting).toBe(false);
    unmount();
  });

  it("returns the unsent message through the next read that lands", async () => {
    saved.A = conversation("A", "Question");
    mocks.activeTurn.mockResolvedValue(unacknowledged());
    const { result, unmount } = await open("A");
    mocks.activeTurn.mockResolvedValue(null);
    mocks.conversationSnapshot.mockRejectedValue(new Error("offline"));
    await emit({ epoch: 0, kind: "activeTurnChanged" });
    await waitFor(() =>
      expect(result.current.feedback.error).toContain("stays locked"),
    );
    mocks.conversationSnapshot.mockImplementation(
      async (id: string) => saved[id],
    );
    await act(async () => {
      await result.current.conversation.hydrate("A");
    });
    expect(result.current.persistence.draft).toBe("Question");
    expect(result.current.conversation.submitting).toBe(false);
    await settle();
    expect(mocks.updateSession).not.toHaveBeenCalled();
    unmount();
  });
});

it("keeps an archived conversation opened from another project", async () => {
  saved.Q = conversation("Q");
  saved.Q.session.workspaceId = "project-q";
  saved.Q.session.archivedAt = "earlier";
  saved.B.session.workspaceId = "project-q";
  mocks.listSessions.mockImplementation(async (workspace: string | null) => ({
    sessions: workspace === "project-q" ? [saved.B.session] : [],
  }));
  const hook = renderHook(
    (props: Parameters<typeof useWorkspacePageController>[0]) =>
      useWorkspacePageController(props),
    {
      initialProps: {
        entryTarget: { page: "workspace", projectId: "project-p" },
        entryRequest: 1,
      },
    },
  );
  await settle();
  hook.rerender({
    entryTarget: { page: "workspace", sessionId: "Q" },
    entryRequest: 2,
  });
  await waitFor(() =>
    expect(hook.result.current.navigation.workspaceId).toBe("project-q"),
  );
  await settle();
  expect(hook.result.current.navigation.sessionId).toBe("Q");
  expect(hook.result.current.conversation.snapshot?.session.id).toBe("Q");
  expect(hook.result.current.navigation.sessions).toEqual([saved.B.session]);
  hook.unmount();
});
