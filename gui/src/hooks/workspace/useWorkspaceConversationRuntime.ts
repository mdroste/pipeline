import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
  type RefObject,
  type MutableRefObject,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { conversationClient } from "../../lib/workbench/conversationClient";
import { runtimeClient } from "../../lib/workbench/runtimeClient";
import {
  mergeSessionSnapshot,
  newOperation,
} from "../../lib/workspaceSessionState";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type {
  ConversationSnapshot,
  SessionSnapshot,
  WorkbenchSession,
  WorkbenchEvent,
  WorkspaceModel,
} from "../../lib/workbenchTypes";
import type { WorkspaceSessionPersistence } from "./useWorkspaceSessionPersistence";

/** One identity owner for hydration, submission acknowledgement and terminal events. */
export function useWorkspaceConversationRuntime(
  sessionId: string | null,
  setSessions: Dispatch<SetStateAction<WorkbenchSession[]>>,
) {
  const [snapshot, setSnapshot] = useState<ConversationSnapshot | null>(null);
  const [models, setModels] = useState<WorkspaceModel[]>([]);
  const [model, setModel] = useState("");
  const [runtime, setRuntime] = useState<"codex" | "claude">("codex");
  const [effort, setEffort] = useState("");
  const [draft, setDraft] = useState("");
  const [stream, setStream] = useState("");
  const [pendingUser, setPendingUser] = useState<string | null>(null);
  const [active, setActive] = useState<{
    threadId: string;
    turnId: string;
  } | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [requests, setRequests] = useState<WorkbenchEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const sessionRef = useRef(sessionId);
  const snapshotRef = useRef(snapshot);
  const submissionRef = useRef<PendingSubmission | null>(null);
  const activeRef = useRef(active);
  const submittingRef = useRef(false);
  const completedTurnsRef = useRef(new Map<string, string>());
  const streamBufferRef = useRef("");
  const streamFlushRef = useRef<number | null>(null);
  sessionRef.current = sessionId;
  snapshotRef.current = snapshot;
  activeRef.current = active;

  const draftEditRevision = useRef(0);
  const hydrationGeneration = useRef(0);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const hydrate = useCallback(async (selectedSession = sessionRef.current) => {
    if (!selectedSession) {
      if (!sessionRef.current) setSnapshot(null);
      return;
    }
    const generation = ++hydrationGeneration.current;
    const activeAtStart = activeRef.current;
    const ownerAtStart = submissionRef.current;
    const editRevision = draftEditRevision.current;
    const initialize = snapshotRef.current?.session.id !== selectedSession;
    let next: ConversationSnapshot;
    try {
      next = await conversationClient.conversationSnapshot(selectedSession);
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
      generation !== hydrationGeneration.current ||
      activeRef.current !== activeAtStart ||
      submissionRef.current !== ownerAtStart
    )
      return;
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
    const owner = submissionRef.current;
    const pendingHere = owner?.sessionId === selectedSession;
    setPendingUser(pendingHere ? owner.text : null);
    setSubmitting(pendingHere && !owner.turnId);
    const inProgress = [...next.turns]
      .reverse()
      .find(
        (turn) =>
          !turn.terminalAt &&
          turn.providerTurnId &&
          turn.bindingId === next.activeBinding?.id,
      );
    if (!activeRef.current || (!pendingHere && !inProgress)) {
      setStream("");
      if (streamFlushRef.current !== null)
        window.clearTimeout(streamFlushRef.current);
      streamFlushRef.current = null;
      streamBufferRef.current = "";
    }
    if (initialize && draftEditRevision.current === editRevision)
      setDraft(pendingHere ? "" : (recoveredDraft ?? next.session.draft));
    const overrides = next.session.overrides;
    setRuntime(overrides.runtime === "claude" ? "claude" : "codex");
    setModel(typeof overrides.model === "string" ? overrides.model : "");
    setEffort(typeof overrides.effort === "string" ? overrides.effort : "");
    if (pendingHere && owner.threadId && owner.turnId) {
      setActive({ threadId: owner.threadId, turnId: owner.turnId });
    } else if (inProgress && next.activeBinding) {
      setActive({
        threadId: next.activeBinding.providerThreadId,
        turnId: inProgress.providerTurnId!,
      });
    } else {
      setActive(null);
    }
  }, []);

  // Refresh records without replacing the composer draft.
  const refreshSessionRecord = useCallback(async (id: string) => {
    const latest = await conversationClient.sessionSnapshot(id);
    setSessions((old) =>
      old.map((session) => (session.id === id ? latest.session : session)),
    );
    if (sessionRef.current === id)
      setSnapshot((old) => mergeSessionSnapshot(old, latest));
  }, []);

  const acceptSnapshot = useCallback((next: ConversationSnapshot) => {
    if (sessionRef.current === next.session.id) setSnapshot(next);
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
    stream,
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
    sessionRef,
    snapshotRef,
    submissionRef,
    activeRef,
    submittingRef,
    completedTurnsRef,
    streamBufferRef,
    streamFlushRef,
    draftEditRevision,
    hydrationGeneration,
    draftRef,
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
  const {
    activeRef,
    completedTurnsRef,
    hydrate,
    submissionRef,
    refreshSessionRecord,
    sessionRef,
    setActive,
    setDraft,
    setError,
    setModels,
    setPendingUser,
    setRequests,
    setSnapshot,
    setStream,
    setSubmitting,
    snapshotRef,
    streamBufferRef,
    streamFlushRef,
    submittingRef,
  } = conversation;
  const { persistDraft } = persistence;
  const { finishSubmission } = useWorkbenchEvents({
    activeRef,
    completedTurnsRef,
    hydrate,
    submissionRef,
    persistDraft,
    refreshSessionRecord,
    sessionRef,
    setActive,
    setDraft,
    setError,
    setModels,
    setPendingUser,
    setRequests,
    setSnapshot,
    setStream,
    setSubmitting,
    snapshotRef,
    streamBufferRef,
    streamFlushRef,
    submittingRef,
  });

  return { finishSubmission };
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
    streamFlushRef,
    streamBufferRef,
    setError,
    setPendingUser,
    setStream,
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
  } = conversation;
  const { selectionSaveRef, draftSaveRef, editDraft } = persistence;
  const {
    contextBusyRef,
    followLatestRef,
    setSelectedMessage,
    setJumpTarget,
    setTranscriptPage,
  } = presentation;
  const send = async () => {
    hydrationGeneration.current += 1;
    if (
      !sessionId ||
      snapshotRef.current?.session.id !== sessionId ||
      !draft.trim() ||
      active ||
      submittingRef.current ||
      submissionRef.current ||
      contextBusyRef.current
    )
      return;
    submittingRef.current = true;
    setSubmitting(true);
    const text = draft.trim();
    const submission: PendingSubmission = { sessionId, text };
    submissionRef.current = submission;
    if (streamFlushRef.current !== null)
      window.clearTimeout(streamFlushRef.current);
    streamFlushRef.current = null;
    streamBufferRef.current = "";
    followLatestRef.current = true;
    setSelectedMessage(null);
    setJumpTarget(null);
    setTranscriptPage(0);
    setError(null);
    setPendingUser(text);
    setStream("");
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
      if (!dispatched) {
        submissionRef.current = null;
        if (sessionRef.current === sessionId) {
          setPendingUser(null);
          setDraft(text);
        }
        try {
          localStorage.setItem(`pipeline.pendingDraft.${sessionId}`, text);
        } catch {
          /* Keep the visible draft. */
        }
        if (sessionRef.current === sessionId)
          setError(
            `Message not sent because its settings or draft could not be saved: ${workbenchErrorMessage(cause)}`,
          );
        return;
      }
      if (sessionRef.current === sessionId)
        setError(
          `Pipeline could not confirm whether the message was received. It was preserved as a draft and will not be retried automatically. ${workbenchErrorMessage(cause)}`,
        );
      submissionRef.current = null;
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

type ActiveTurn = { threadId: string; turnId: string };
interface PendingSubmission {
  sessionId: string;
  text: string;
  threadId?: string;
  turnId?: string;
  epoch?: number;
}
const terminalKey = (epoch: unknown, thread: unknown, turn: unknown) =>
  `${String(epoch)}:${String(thread)}:${String(turn)}`;

interface WorkbenchEventBridge {
  activeRef: MutableRefObject<ActiveTurn | null>;
  completedTurnsRef: MutableRefObject<Map<string, string>>;
  hydrate: (sessionId?: string | null) => Promise<void>;
  submissionRef: MutableRefObject<PendingSubmission | null>;
  persistDraft: (sessionId: string, draft: string) => Promise<SessionSnapshot>;
  refreshSessionRecord: (sessionId: string) => Promise<void>;
  sessionRef: MutableRefObject<string | null>;
  setActive: Dispatch<SetStateAction<ActiveTurn | null>>;
  setDraft: Dispatch<SetStateAction<string>>;
  setError: Dispatch<SetStateAction<string | null>>;
  setModels: Dispatch<SetStateAction<WorkspaceModel[]>>;
  setPendingUser: Dispatch<SetStateAction<string | null>>;
  setRequests: Dispatch<SetStateAction<WorkbenchEvent[]>>;
  setSnapshot: Dispatch<SetStateAction<ConversationSnapshot | null>>;
  setStream: Dispatch<SetStateAction<string>>;
  setSubmitting: Dispatch<SetStateAction<boolean>>;
  snapshotRef: MutableRefObject<ConversationSnapshot | null>;
  streamBufferRef: MutableRefObject<string>;
  streamFlushRef: MutableRefObject<number | null>;
  submittingRef: MutableRefObject<boolean>;
}

/** Owns the native runtime event subscription and terminal-turn cleanup. */
function useWorkbenchEvents({
  activeRef,
  completedTurnsRef,
  hydrate,
  submissionRef,
  persistDraft,
  refreshSessionRecord,
  sessionRef,
  setActive,
  setDraft,
  setError,
  setModels,
  setPendingUser,
  setRequests,
  setSnapshot,
  setStream,
  setSubmitting,
  snapshotRef,
  streamBufferRef,
  streamFlushRef,
  submittingRef,
}: WorkbenchEventBridge) {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("chatgpt:account-changed", () => {
      if (snapshotRef.current?.session.overrides.runtime === "claude") return;
      void runtimeClient
        .accountState()
        .then(async (account) => {
          if (
            disposed ||
            snapshotRef.current?.session.overrides.runtime === "claude"
          )
            return;
          if (account.status === "chatgpt") {
            const catalog = await runtimeClient.modelCatalog();
            if (
              !disposed &&
              snapshotRef.current?.session.overrides.runtime !== "claude"
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

  const finishSubmission = useCallback(
    (status: string) => {
      const owner = submissionRef.current;
      if (!owner) return;
      submissionRef.current = null;
      submittingRef.current = false;
      const selected = sessionRef.current === owner.sessionId;
      if (status !== "completed") {
        // Persist to the originating conversation even after the user navigates.
        try {
          localStorage.setItem(
            `pipeline.pendingDraft.${owner.sessionId}`,
            owner.text,
          );
        } catch {
          /* Optional cache. */
        }
        void persistDraft(owner.sessionId, owner.text)
          .then((next) => {
            if (sessionRef.current === owner.sessionId)
              setSnapshot((old) => mergeSessionSnapshot(old, next));
          })
          .catch(() => undefined);
        if (selected) {
          setDraft(owner.text);
          setError(
            `The turn ended with status ${status}. Its message was restored to this conversation.`,
          );
        }
      } else {
        void runtimeClient
          .reconcileSession(owner.sessionId)
          .catch(() => false)
          .finally(() => {
            if (sessionRef.current === owner.sessionId)
              void hydrate(owner.sessionId);
          });
      }
      if (selected) {
        if (streamFlushRef.current !== null)
          window.clearTimeout(streamFlushRef.current);
        streamFlushRef.current = null;
        streamBufferRef.current = "";
        activeRef.current = null;
        setSubmitting(false);
        setActive(null);
        setPendingUser(null);
        setStream("");
      }
    },
    [
      submissionRef,
      submittingRef,
      sessionRef,
      persistDraft,
      setSnapshot,
      setDraft,
      setError,
      hydrate,
      streamFlushRef,
      streamBufferRef,
      activeRef,
      setSubmitting,
      setActive,
      setPendingUser,
      setStream,
    ],
  );

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<WorkbenchEvent>("workbench:event", ({ payload }) => {
      if (disposed) return;
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
          setStream("");
          const selected = sessionRef.current;
          // Hydrate only when reconciliation settled the turn: hydrating an
          // unchanged snapshot would re-adopt the dead turn.
          if (selected)
            void runtimeClient
              .reconcileSession(selected)
              .then((changed) => (changed ? hydrate(selected) : undefined))
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
        payload.kind === "serverRequest" &&
        matchesThread(payload.params?.threadId)
      )
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
        matchesThread(payload.threadId) &&
        (!owner || (ownsTurn && owner.sessionId === sessionRef.current))
      ) {
        streamBufferRef.current += String(payload.delta ?? "");
        if (streamFlushRef.current === null)
          streamFlushRef.current = window.setTimeout(() => {
            const delta = streamBufferRef.current;
            streamBufferRef.current = "";
            streamFlushRef.current = null;
            if (delta && sessionRef.current === selectedSnapshot?.session.id)
              setStream((old) => old + delta);
          }, STREAM_FLUSH_MS);
      }

      if (payload.kind === "turnCompleted") {
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
          setStream("");
          const selected = sessionRef.current;
          if (selected)
            void runtimeClient
              .reconcileSession(selected)
              .catch(() => false)
              .finally(() => void hydrate(selected));
        }
      }
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
      if (streamFlushRef.current !== null)
        window.clearTimeout(streamFlushRef.current);
      streamFlushRef.current = null;
      streamBufferRef.current = "";
    };
  }, [
    finishSubmission,
    activeRef,
    completedTurnsRef,
    hydrate,
    submissionRef,
    persistDraft,
    refreshSessionRecord,
    sessionRef,
    setActive,
    setDraft,
    setError,
    setModels,
    setPendingUser,
    setRequests,
    setSnapshot,
    setStream,
    setSubmitting,
    snapshotRef,
    streamBufferRef,
    streamFlushRef,
    submittingRef,
  ]);
  return { finishSubmission };
}
