import {
  conversationProvider,
  type ConversationProvider,
} from "../../lib/workspaceProviders";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
  type RefObject,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { conversationClient } from "../../lib/workbench/conversationClient";
import { runtimeClient } from "../../lib/workbench/runtimeClient";
import {
  mergeSessionSnapshot,
  newOperation,
} from "../../lib/workspaceSessionState";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { createStreamStore } from "./streamStore";
import {
  addTurnText,
  transcriptHasPrompt,
  unsavedTurnText,
  type TurnText,
} from "./turnText";
import { useActiveTurn } from "./useActiveTurn";
import {
  busyMessage,
  terminalKey,
  useAdoptedSubmission,
} from "./useAdoptedSubmission";
import type {
  ActiveTurn,
  ConversationSnapshot,
  SessionSnapshot,
  WorkbenchSession,
  WorkbenchEvent,
  WorkspaceModel,
} from "../../lib/workbenchTypes";
import type { WorkspaceSessionPersistence } from "./useWorkspaceSessionPersistence";

const readActiveTurn = () => runtimeClient.activeTurn();

/** One identity owner for hydration, submission acknowledgement and terminal events. */
export function useWorkspaceConversationRuntime(
  sessionId: string | null,
  setSessions: Dispatch<SetStateAction<WorkbenchSession[]>>,
) {
  const [snapshot, setSnapshot] = useState<ConversationSnapshot | null>(null);
  const [models, setModels] = useState<WorkspaceModel[]>([]);
  const [model, setModel] = useState("");
  const [runtime, setRuntime] = useState<ConversationProvider>("codex");
  const [effort, setEffort] = useState("");
  const [draft, setDraft] = useState("");
  const [streamStore] = useState(createStreamStore);
  const setStream = streamStore.set;
  const [pendingUser, setPendingUser] = useState<string | null>(null);
  const [active, setActive] = useState<{
    threadId: string;
    turnId: string;
  } | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [requests, setRequests] = useState<WorkbenchEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // Mirrors submissionRef for rendering: a message may still be running in
  // a conversation the researcher has since left.
  const [submissionOwner, setSubmissionOwner] =
    useState<SubmissionOwner | null>(null);
  const { activeTurn, refreshActiveTurn, observeActiveTurn } =
    useActiveTurn(readActiveTurn);
  const sessionRef = useRef(sessionId);
  const snapshotRef = useRef(snapshot);
  const submissionRef = useRef<PendingSubmission | null>(null);
  const activeRef = useRef(active);
  const submittingRef = useRef(false);
  const completedTurnsRef = useRef(new Map<string, string>());
  const streamBufferRef = useRef("");
  const streamFlushRef = useRef<number | null>(null);
  const streamItemRef = useRef<string | null>(null);
  const turnTextRef = useRef<TurnText | null>(null);
  // Live file-change items, so an approval can show the files it would change.
  const fileChangesRef = useRef(new Map<string, Record<string, unknown>>());
  sessionRef.current = sessionId;
  snapshotRef.current = snapshot;
  activeRef.current = active;

  const draftEditRevision = useRef(0);
  const hydrationGeneration = useRef(0);
  // A conversation whose saved draft must reach the composer on the next load
  // that lands, whichever caller's it is.
  const draftOwedRef = useRef<string | null>(null);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const clearStream = useCallback(() => {
    if (streamFlushRef.current !== null)
      window.clearTimeout(streamFlushRef.current);
    streamFlushRef.current = null;
    streamBufferRef.current = "";
    streamItemRef.current = null;
    setStream("");
  }, []);
  const hydrate = useCallback(
    async function load(
      selectedSession = sessionRef.current,
      reloadDraft = false,
    ): Promise<void> {
      if (!selectedSession) {
        if (!sessionRef.current) setSnapshot(null);
        return;
      }
      // A late hydration for a conversation the user has left must not
      // invalidate the load of the conversation now on screen.
      if (selectedSession !== sessionRef.current) return;
      if (reloadDraft) draftOwedRef.current = selectedSession;
      const generation = ++hydrationGeneration.current;
      const activeAtStart = activeRef.current;
      const ownerAtStart = submissionRef.current;
      const editRevision = draftEditRevision.current;
      const initialize = snapshotRef.current?.session.id !== selectedSession;
      let next: ConversationSnapshot;
      let hostTurn: ActiveTurn | null | undefined;
      try {
        [next, hostTurn] = await Promise.all([
          conversationClient.conversationSnapshot(selectedSession),
          refreshActiveTurn(),
        ]);
      } catch (cause) {
        if (
          sessionRef.current === selectedSession &&
          generation === hydrationGeneration.current
        )
          setError(workbenchErrorMessage(cause));
        return;
      }
      if (
        sessionRef.current !== selectedSession ||
        generation !== hydrationGeneration.current
      )
        return;
      if (
        activeRef.current !== activeAtStart ||
        submissionRef.current !== ownerAtStart
      ) {
        // That change may belong to the conversation just left. Nothing else
        // retries, so a conversation still waiting for its first load reads
        // again rather than staying blank.
        if (snapshotRef.current?.session.id !== selectedSession)
          await load(selectedSession);
        return;
      }
      const current = snapshotRef.current;
      if (
        current?.session.id === selectedSession &&
        (current.sequence > next.sequence ||
          current.session.revision > next.session.revision)
      )
        return;
      setSnapshot(next);
      let recoveredDraft: string | null = null;
      try {
        recoveredDraft = localStorage.getItem(
          `pipeline.pendingDraft.${selectedSession}`,
        );
      } catch {
        /* Optional recovery cache. */
      }
      const turnHere =
        hostTurn?.sessionId === selectedSession ? hostTurn : null;
      // A send whose page closed before its acknowledgement. Its text is still
      // the saved draft, and offering that again would let it be sent twice.
      if (
        !submissionRef.current &&
        turnHere?.composer &&
        turnHere.setup &&
        !turnHere.turnId
      ) {
        submissionRef.current = {
          sessionId: selectedSession,
          text: (recoveredDraft ?? next.session.draft).trim(),
          adopted: true,
        };
        submittingRef.current = true;
        setSubmissionOwner({ sessionId: selectedSession, threadId: null });
      }
      const owner = submissionRef.current;
      const pendingHere = owner?.sessionId === selectedSession;
      setPendingUser(
        pendingHere && !transcriptHasPrompt(next, owner) ? owner.text : null,
      );
      setSubmitting(pendingHere && !owner.turnId);
      const inProgress = [...next.turns]
        .reverse()
        .find(
          (turn) =>
            !turn.terminalAt &&
            turn.providerTurnId &&
            turn.bindingId === next.activeBinding?.id,
        );
      if (!activeRef.current || (!pendingHere && !inProgress)) clearStream();
      // An acknowledged send's text is in the thread but can stay the saved
      // draft for a moment. Offering it again would let it be sent twice.
      const sentHere = Boolean(turnHere?.composer && turnHere.turnId);
      if (
        (initialize || draftOwedRef.current === selectedSession) &&
        draftEditRevision.current === editRevision
      ) {
        draftOwedRef.current = null;
        setDraft(
          pendingHere || sentHere ? "" : (recoveredDraft ?? next.session.draft),
        );
      }
      const overrides = next.session.overrides;
      setRuntime(conversationProvider(overrides.runtime));
      setModel(typeof overrides.model === "string" ? overrides.model : "");
      setEffort(typeof overrides.effort === "string" ? overrides.effort : "");
      let running: { threadId: string; turnId: string } | null = null;
      if (pendingHere && owner.threadId && owner.turnId) {
        running = { threadId: owner.threadId, turnId: owner.turnId };
      } else if (inProgress && next.activeBinding) {
        running = {
          threadId: next.activeBinding.providerThreadId,
          turnId: inProgress.providerTurnId!,
        };
      } else if (turnHere?.threadId && turnHere.turnId && !turnHere.automated) {
        running = { threadId: turnHere.threadId, turnId: turnHere.turnId };
      }
      setActive(running);
      // The answer so far, including what arrived while this conversation was
      // not on screen. Only the part the transcript doesn't show yet stays live.
      const said = turnTextRef.current;
      if (running && said?.turnId === running.turnId) {
        const { text, itemId } = unsavedTurnText(said, next.items);
        clearStream();
        streamItemRef.current = itemId;
        setStream(text);
      }
    },
    [clearStream, refreshActiveTurn, setStream],
  );

  // Refresh records without replacing the composer draft.
  const refreshSessionRecord = useCallback(async (id: string) => {
    const latest = await conversationClient.sessionSnapshot(id);
    setSessions((old) =>
      old.map((session) => (session.id === id ? latest.session : session)),
    );
    if (sessionRef.current === id)
      setSnapshot((old) => mergeSessionSnapshot(old, latest));
  }, []);

  // A session save carries no transcript, so it never replaces the one on screen.
  const acceptSnapshot = useCallback((next: SessionSnapshot) => {
    if (sessionRef.current !== next.session.id) return;
    setSnapshot((old) =>
      "items" in next
        ? (next as ConversationSnapshot)
        : mergeSessionSnapshot(old, next),
    );
  }, []);
  return {
    sessionId,
    snapshot,
    setSnapshot,
    models,
    setModels,
    model,
    setModel,
    runtime,
    setRuntime,
    effort,
    setEffort,
    draft,
    setDraft,
    streamStore,
    setStream,
    pendingUser,
    setPendingUser,
    active,
    setActive,
    submitting,
    setSubmitting,
    requests,
    setRequests,
    error,
    setError,
    notice,
    setNotice,
    submissionOwner,
    setSubmissionOwner,
    activeTurn,
    refreshActiveTurn,
    observeActiveTurn,
    sessionRef,
    snapshotRef,
    submissionRef,
    activeRef,
    submittingRef,
    completedTurnsRef,
    streamBufferRef,
    streamFlushRef,
    streamItemRef,
    turnTextRef,
    fileChangesRef,
    draftEditRevision,
    hydrationGeneration,
    draftOwedRef,
    draftRef,
    clearStream,
    hydrate,
    refreshSessionRecord,
    acceptSnapshot,
  };
}
export type WorkspaceConversationRuntime = ReturnType<
  typeof useWorkspaceConversationRuntime
>;

/** Registered at the original event-subscription phase, after reconciliation and autosave. */
export function useWorkspaceConversationEvents(
  conversation: WorkspaceConversationRuntime,
  persistence: WorkspaceSessionPersistence,
) {
  return useWorkbenchEvents({
    ...conversation,
    persistDraft: persistence.persistDraft,
  });
}

interface SubmissionPresentation {
  contextBusyRef: RefObject<boolean>;
  followLatestRef: RefObject<boolean>;
  setSelectedMessage: Dispatch<SetStateAction<string | null>>;
  setJumpTarget: Dispatch<SetStateAction<string | null>>;
  setTranscriptPage: Dispatch<SetStateAction<number>>;
}
/** Create the send operation using the same identity and the shared persistence barriers. */
export function createWorkspaceSubmission(
  conversation: WorkspaceConversationRuntime,
  persistence: WorkspaceSessionPersistence,
  finishSubmission: (status: string) => void,
  presentation: SubmissionPresentation,
) {
  const {
    hydrationGeneration,
    sessionId,
    snapshotRef,
    draft,
    active,
    submittingRef,
    submissionRef,
    setSubmitting,
    setSubmissionOwner,
    clearStream,
    setError,
    setNotice,
    setPendingUser,
    setDraft,
    draftEditRevision,
    model,
    effort,
    completedTurnsRef,
    sessionRef,
    activeRef,
    setActive,
    setRequests,
    hydrate,
    refreshActiveTurn,
  } = conversation;
  const { selectionSaveRef, draftSaveRef, editDraft, persistDraft } =
    persistence;
  const {
    contextBusyRef,
    followLatestRef,
    setSelectedMessage,
    setJumpTarget,
    setTranscriptPage,
  } = presentation;
  const send = async () => {
    if (
      !sessionId ||
      snapshotRef.current?.session.id !== sessionId ||
      !draft.trim()
    )
      return;
    const running = submissionRef.current;
    if (running && running.sessionId !== sessionId) {
      setError(
        "Message not sent: a response is still running in another conversation. Send it when that response finishes, or open that conversation to stop it.",
      );
      return;
    }
    if (active || submittingRef.current || running || contextBusyRef.current)
      return;
    hydrationGeneration.current += 1;
    submittingRef.current = true;
    setSubmitting(true);
    const text = draft.trim();
    const submission: PendingSubmission = { sessionId, text };
    submissionRef.current = submission;
    setSubmissionOwner({ sessionId, threadId: null });
    clearStream();
    followLatestRef.current = true;
    setSelectedMessage(null);
    setJumpTarget(null);
    setTranscriptPage(0);
    setError(null);
    setNotice(null);
    setPendingUser(text);
    setDraft("");
    try {
      localStorage.removeItem(`pipeline.pendingDraft.${sessionId}`);
    } catch {
      /* The host retains submission recovery. */
    }
    const editRevisionAtSend = draftEditRevision.current; // see catch
    let dispatched = false;
    try {
      await selectionSaveRef.current;
      await draftSaveRef.current;
      dispatched = true;
      const result = await runtimeClient.sendTurn({
        sessionId,
        text,
        clientSubmissionId: newOperation("submission"),
        model: model || null,
        effort: effort || null,
      });
      Object.assign(submission, {
        threadId: result.threadId,
        turnId: result.turnId,
        epoch: result.epoch,
      });
      setSubmissionOwner({ sessionId, threadId: result.threadId });
      // With nothing typed since, what the host left saved is the sent message,
      // which would return on reopen and could be sent twice.
      if (
        result.draftCleared === false &&
        draftEditRevision.current === editRevisionAtSend
      )
        void persistDraft(sessionId, "").catch(() => undefined);
      const key = terminalKey(result.epoch, result.threadId, result.turnId);
      const terminal =
        completedTurnsRef.current.get(key) ??
        completedTurnsRef.current.get(terminalKey(result.epoch, "", ""));
      if (terminal) {
        completedTurnsRef.current.delete(key);
        finishSubmission(terminal);
      } else if (sessionRef.current === sessionId) {
        const turn = { threadId: result.threadId, turnId: result.turnId };
        activeRef.current = turn;
        setActive(turn);
        setSubmitting(false);
      }
      submittingRef.current = false;
      void runtimeClient
        .pendingRequests()
        .then((pending) => setRequests(pending))
        .catch(() => undefined);
    } catch (cause) {
      submittingRef.current = false;
      if (sessionRef.current === sessionId) setSubmitting(false);
      submissionRef.current = null;
      setSubmissionOwner(null);
      // The host marks failures that happened before the message left
      // Pipeline, so those are restored as unsent rather than uncertain.
      const code = (cause as { code?: unknown } | null)?.code;
      const notSent = code === "turn_not_sent";
      const busy = code === "turn_busy";
      if (!dispatched || notSent || busy) {
        if (sessionRef.current === sessionId) {
          setPendingUser(null);
          setDraft(text);
        }
        try {
          localStorage.setItem(`pipeline.pendingDraft.${sessionId}`, text);
        } catch {
          /* Keep the visible draft. */
        }
        const message = busy
          ? busyMessage(await refreshActiveTurn(), sessionId)
          : notSent
            ? `Message not sent: ${workbenchErrorMessage(cause)}`
            : `Message not sent because its settings or draft could not be saved: ${workbenchErrorMessage(cause)}`;
        if (sessionRef.current === sessionId) setError(message);
        return;
      }
      if (sessionRef.current === sessionId)
        setError(
          `Pipeline could not confirm whether the message was received. It was preserved as a draft and will not be retried automatically. ${workbenchErrorMessage(cause)}`,
        );
      if (sessionRef.current === sessionId) setPendingUser(null);
      await hydrate(sessionId);
      if (
        sessionRef.current === sessionId &&
        draftEditRevision.current === editRevisionAtSend
      )
        editDraft(text);
    }
  };

  return send;
}

// Private event bridge: registration stays separate so the page preserves effect order.
const STREAM_FLUSH_MS = 50;
const MAX_STREAM_FLUSH_MS = 300;
const FILE_CHANGE_LIMIT = 32;
/** Each flush parses the whole answer, so a long answer flushes less often. */
const streamFlushDelay = (length: number) =>
  Math.min(MAX_STREAM_FLUSH_MS, Math.max(STREAM_FLUSH_MS, length / 64));

interface PendingSubmission {
  sessionId: string;
  text: string;
  threadId?: string;
  turnId?: string;
  epoch?: number;
  /** Found running after its page closed, so no promise reports its outcome. */
  adopted?: boolean;
}
export interface SubmissionOwner {
  sessionId: string;
  threadId: string | null;
}
/** A plain sentence for a response that ended without completing. */
function endedMessage(status: string) {
  if (status === "interrupted") return "Response stopped.";
  if (status === "connection closed")
    return "The connection to the assistant closed before the response finished.";
  if (status === "failed") return "The response failed.";
  return "The response did not finish.";
}

type WorkbenchEventBridge = Pick<
  WorkspaceConversationRuntime,
  | "activeRef"
  | "activeTurn"
  | "clearStream"
  | "completedTurnsRef"
  | "draftEditRevision"
  | "draftOwedRef"
  | "fileChangesRef"
  | "hydrate"
  | "observeActiveTurn"
  | "refreshActiveTurn"
  | "refreshSessionRecord"
  | "sessionRef"
  | "setActive"
  | "setDraft"
  | "setError"
  | "setModels"
  | "setNotice"
  | "setPendingUser"
  | "setRequests"
  | "setStream"
  | "setSubmissionOwner"
  | "setSubmitting"
  | "snapshotRef"
  | "streamBufferRef"
  | "streamFlushRef"
  | "streamItemRef"
  | "streamStore"
  | "submissionOwner"
  | "submissionRef"
  | "submittingRef"
  | "turnTextRef"
> & {
  persistDraft: (sessionId: string, draft: string) => Promise<SessionSnapshot>;
};

/** Owns the native runtime event subscription and terminal-turn cleanup. */
function useWorkbenchEvents({
  activeRef,
  activeTurn,
  clearStream,
  completedTurnsRef,
  draftEditRevision,
  draftOwedRef,
  fileChangesRef,
  hydrate,
  observeActiveTurn,
  persistDraft,
  refreshActiveTurn,
  refreshSessionRecord,
  sessionRef,
  setActive,
  setDraft,
  setError,
  setModels,
  setNotice,
  setPendingUser,
  setRequests,
  setStream,
  setSubmissionOwner,
  setSubmitting,
  snapshotRef,
  streamBufferRef,
  streamFlushRef,
  streamItemRef,
  streamStore,
  submissionOwner,
  submissionRef,
  submittingRef,
  turnTextRef,
}: WorkbenchEventBridge) {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("chatgpt:account-changed", () => {
      if (
        conversationProvider(snapshotRef.current?.session.overrides.runtime) !==
        "codex"
      )
        return;
      void runtimeClient
        .accountState()
        .then(async (account) => {
          if (
            disposed ||
            conversationProvider(
              snapshotRef.current?.session.overrides.runtime,
            ) !== "codex"
          )
            return;
          if (account.status === "chatgpt") {
            const catalog = await runtimeClient.modelCatalog();
            if (
              !disposed &&
              conversationProvider(
                snapshotRef.current?.session.overrides.runtime,
              ) === "codex"
            ) {
              setModels(catalog.models);
              setError(null);
            }
          } else {
            setModels([]);
          }
        })
        .catch(() => undefined);
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [setModels, setError]);

  // Lost events never arrive, so read what they would have said. The host has
  // already recovered the turn itself by the time this is sent.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("workbench:event-lagged", () => {
      if (disposed) return;
      void runtimeClient
        .pendingRequests()
        .then((pending) => {
          if (!disposed) setRequests(pending);
        })
        .catch(() => undefined);
      void hydrate();
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [hydrate, setRequests]);

  // Show text that arrived just before a turn ended instead of dropping it.
  const flushStream = useCallback(() => {
    if (streamFlushRef.current !== null)
      window.clearTimeout(streamFlushRef.current);
    streamFlushRef.current = null;
    const rest = streamBufferRef.current;
    streamBufferRef.current = "";
    if (rest) setStream((old) => old + rest);
  }, [streamFlushRef, streamBufferRef, setStream]);

  // The finished prompt and streamed answer stay on screen until the
  // refreshed transcript replaces them, so the exchange never blinks out.
  const settleTurn = useCallback(
    async (selected: string) => {
      await runtimeClient.reconcileSession(selected).catch(() => false);
      if (sessionRef.current !== selected) return;
      await hydrate(selected);
      // hydrate keeps the screen when it fails or is superseded; never leave
      // a finished exchange as a live bubble.
      if (
        sessionRef.current === selected &&
        !submissionRef.current &&
        !activeRef.current
      ) {
        setPendingUser(null);
        clearStream();
      }
    },
    [
      hydrate,
      sessionRef,
      submissionRef,
      activeRef,
      setPendingUser,
      clearStream,
    ],
  );

  // Put a message back in its composer only when its turn was never
  // acknowledged. An acknowledged turn holds the message in its thread even
  // when the saved transcript (the newest items only) no longer reaches it,
  // and restoring it would let the researcher send it twice.
  const restorePrompt = useCallback(
    (owner: PendingSubmission, status: string, editRevision: number) => {
      if (owner.turnId) return;
      const selected = sessionRef.current === owner.sessionId;
      if (selected && draftEditRevision.current !== editRevision) {
        setError(
          `${endedMessage(status)} Its message is not in the conversation.`,
        );
        return;
      }
      // Persist to the originating conversation even after the user navigates.
      try {
        localStorage.setItem(
          `pipeline.pendingDraft.${owner.sessionId}`,
          owner.text,
        );
      } catch {
        /* Optional cache. */
      }
      void persistDraft(owner.sessionId, owner.text).catch(() => undefined);
      if (selected) {
        setDraft(owner.text);
        setError(
          `${endedMessage(status)} Its message was restored to this conversation.`,
        );
      }
    },
    [sessionRef, draftEditRevision, setError, persistDraft, setDraft],
  );

  const finishSubmission = useCallback(
    (status: string) => {
      const owner = submissionRef.current;
      if (!owner) return;
      submissionRef.current = null;
      submittingRef.current = false;
      setSubmissionOwner(null);
      const selected = sessionRef.current === owner.sessionId;
      const editRevision = draftEditRevision.current;
      if (selected) {
        flushStream();
        activeRef.current = null;
        setSubmitting(false);
        setActive(null);
        if (status === "interrupted") setNotice(endedMessage(status));
        else if (status !== "completed") setError(endedMessage(status));
      }
      void settleTurn(owner.sessionId)
        .then(() => {
          if (status !== "completed")
            restorePrompt(owner, status, editRevision);
        })
        .catch(() => undefined);
    },
    [
      submissionRef,
      submittingRef,
      setSubmissionOwner,
      sessionRef,
      draftEditRevision,
      flushStream,
      activeRef,
      setSubmitting,
      setActive,
      setNotice,
      setError,
      settleTurn,
      restorePrompt,
    ],
  );

  const observeAdoptedSend = useAdoptedSubmission({
    activeRef,
    activeTurn,
    completedTurnsRef,
    draftOwedRef,
    finishSubmission,
    hydrate,
    sessionRef,
    setActive,
    setError,
    setPendingUser,
    setSubmissionOwner,
    setSubmitting,
    submissionOwner,
    submissionRef,
    submittingRef,
  });

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<WorkbenchEvent>("workbench:event", ({ payload }) => {
      if (disposed) return;
      observeAdoptedSend(payload);
      observeActiveTurn(payload);
      const owner = submissionRef.current;
      const selectedSnapshot =
        snapshotRef.current?.session.id === sessionRef.current
          ? snapshotRef.current
          : null;
      const expectedThread =
        activeRef.current?.threadId ??
        selectedSnapshot?.activeBinding?.providerThreadId;
      const matchesThread = (threadId: unknown) =>
        !!expectedThread && threadId === expectedThread;
      const ownsTurn =
        !!owner?.threadId &&
        owner.threadId === payload.threadId &&
        owner.turnId === payload.turnId &&
        owner.epoch === payload.epoch;

      if (payload.kind === "connectionClosed") {
        turnTextRef.current = null;
        completedTurnsRef.current.set(
          terminalKey(payload.epoch, "", ""),
          "connection closed",
        );
        if (owner?.epoch === payload.epoch)
          finishSubmission("connection closed");
        else if (!owner && activeRef.current) {
          // A turn adopted from hydration cannot finish once its connection
          // is gone; leaving it active locks the composer until navigation.
          activeRef.current = null;
          setActive(null);
          clearStream();
          const selected = sessionRef.current;
          // Hydrate only when reconciliation settled the turn: hydrating an
          // unchanged snapshot would re-adopt the dead turn.
          if (selected)
            void runtimeClient
              .reconcileSession(selected)
              .then((changed) =>
                changed && sessionRef.current === selected
                  ? hydrate(selected)
                  : undefined,
              )
              .catch(() => undefined);
        }
        setRequests((old) =>
          old.filter((request) => request.epoch !== payload.epoch),
        );
      }

      // An automation may start a turn on the viewed conversation's thread;
      // adopt it so its deltas end in a real message, not a frozen bubble.
      if (
        payload.kind === "turnStarted" &&
        !owner &&
        !activeRef.current &&
        matchesThread(payload.threadId) &&
        typeof payload.turnId === "string"
      ) {
        const turn = {
          threadId: String(payload.threadId),
          turnId: payload.turnId,
        };
        activeRef.current = turn;
        setActive(turn);
        setPendingUser(null);
        clearStream();
      }

      if (
        payload.kind === "sessionTitleUpdated" &&
        typeof payload.sessionId === "string"
      )
        void refreshSessionRecord(payload.sessionId).catch(() => undefined);
      if (
        payload.kind === "sessionTitleFailed" &&
        payload.sessionId === sessionRef.current
      )
        setError(
          `Automatic title unavailable: ${workbenchErrorMessage(payload.error ?? "")}. Rename the conversation from its menu.`,
        );

      if (
        (payload.kind === "itemStarted" || payload.kind === "itemCompleted") &&
        payload.itemKind === "fileChange" &&
        typeof payload.itemId === "string" &&
        payload.item &&
        typeof payload.item === "object"
      ) {
        const changes = fileChangesRef.current;
        changes.delete(payload.itemId);
        changes.set(payload.itemId, payload.item as Record<string, unknown>);
        if (changes.size > FILE_CHANGE_LIMIT)
          changes.delete(changes.keys().next().value!);
      }

      // Keep every thread's requests: one may belong to a running turn in a
      // conversation the researcher has left. The page filters by thread.
      if (payload.kind === "serverRequest")
        setRequests((old) =>
          old.some(
            (item) =>
              item.requestId === payload.requestId &&
              item.epoch === payload.epoch,
          )
            ? old
            : [...old, payload],
        );
      if (payload.kind === "serverRequestResolved")
        setRequests((old) =>
          old.filter(
            (item) =>
              item.requestId !== payload.requestId ||
              item.epoch !== payload.epoch,
          ),
        );

      if (
        payload.kind === "agentMessageDelta" &&
        typeof payload.turnId === "string"
      )
        turnTextRef.current = addTurnText(
          turnTextRef.current,
          payload.turnId,
          typeof payload.itemId === "string" ? payload.itemId : null,
          String(payload.delta ?? ""),
        );

      if (
        payload.kind === "agentMessageDelta" &&
        matchesThread(payload.threadId) &&
        (!owner || (ownsTurn && owner.sessionId === sessionRef.current))
      ) {
        // A turn can write several assistant messages (a preamble before a
        // tool call, then the answer); keep them apart as the transcript does.
        const itemId =
          typeof payload.itemId === "string" ? payload.itemId : null;
        const separator =
          itemId && streamItemRef.current && itemId !== streamItemRef.current
            ? "\n\n"
            : "";
        if (itemId) streamItemRef.current = itemId;
        streamBufferRef.current += separator + String(payload.delta ?? "");
        if (streamFlushRef.current === null)
          streamFlushRef.current = window.setTimeout(() => {
            const delta = streamBufferRef.current;
            streamBufferRef.current = "";
            streamFlushRef.current = null;
            if (delta && sessionRef.current === selectedSnapshot?.session.id)
              setStream((old) => old + delta);
          }, streamFlushDelay(streamStore.get().length));
      }

      if (payload.kind === "turnCompleted") {
        if (turnTextRef.current?.turnId === payload.turnId)
          turnTextRef.current = null;
        const key = terminalKey(
          payload.epoch,
          payload.threadId,
          payload.turnId,
        );
        completedTurnsRef.current.set(key, String(payload.status ?? "failed"));
        if (completedTurnsRef.current.size > 100) {
          const oldest = completedTurnsRef.current.keys().next().value;
          if (oldest) completedTurnsRef.current.delete(oldest);
        }
        if (ownsTurn) finishSubmission(String(payload.status ?? "failed"));
        else if (
          !owner &&
          matchesThread(payload.threadId) &&
          (!activeRef.current || activeRef.current.turnId === payload.turnId)
        ) {
          activeRef.current = null;
          setActive(null);
          flushStream();
          const selected = sessionRef.current;
          if (selected) void settleTurn(selected);
        }
      }
    }).then((dispose) => {
      if (disposed) dispose();
      else {
        unlisten = dispose;
        // Read after subscribing, so no change can fall between the two.
        void refreshActiveTurn();
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
      if (streamFlushRef.current !== null)
        window.clearTimeout(streamFlushRef.current);
      streamFlushRef.current = null;
      streamBufferRef.current = "";
      streamItemRef.current = null;
    };
  }, [
    finishSubmission,
    activeRef,
    clearStream,
    completedTurnsRef,
    fileChangesRef,
    flushStream,
    hydrate,
    observeActiveTurn,
    observeAdoptedSend,
    refreshActiveTurn,
    refreshSessionRecord,
    sessionRef,
    setActive,
    setError,
    setPendingUser,
    setRequests,
    setStream,
    settleTurn,
    snapshotRef,
    streamBufferRef,
    streamFlushRef,
    streamItemRef,
    streamStore,
    submissionRef,
    turnTextRef,
  ]);
  return { finishSubmission };
}
