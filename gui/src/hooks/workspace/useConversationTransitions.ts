import type { DecodedConversationEvent } from "../../lib/workbench/events";
import { conversationProvider } from "../../lib/workspaceProviders";
import { useCallback, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { runtimeClient } from "../../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { turnFailureReason, withReason } from "../../lib/workspaceTurnLabels";
import { reportConnectionWarning, reportProblem } from "./reportProblem";
import { addTurnText } from "./turnText";
import { terminalKey, useAdoptedSubmission } from "./useAdoptedSubmission";
import type { SessionSnapshot } from "../../lib/workbench/sessionTypes";
import type { WorkspaceConversationRuntime } from "./useWorkspaceConversationRuntime";
import type { PendingSubmission } from "./conversationRuntimeTypes";

// Private event bridge: registration stays separate so the page preserves effect order.
const STREAM_FLUSH_MS = 50;
const MAX_STREAM_FLUSH_MS = 300;
const FILE_CHANGE_LIMIT = 32;
/** Each flush parses the whole answer, so a long answer flushes less often. */
const streamFlushDelay = (length: number) =>
  Math.min(MAX_STREAM_FLUSH_MS, Math.max(STREAM_FLUSH_MS, length / 64));

/** A plain sentence for a response that ended without completing. */
function endedMessage(status: string, reason?: string | null) {
  if (status === "interrupted") return "Response stopped.";
  if (status === "connection closed")
    return "The connection to the assistant closed before the response finished.";
  if (status === "failed") return withReason("The response failed", reason);
  return withReason("The response did not finish", reason);
}

export type WorkbenchEventBridge = Pick<
  WorkspaceConversationRuntime,
  | "activeRef"
  | "activeTurn"
  | "clearStream"
  | "completedTurnsRef"
  | "draftEditRevision"
  | "draftOwedRef"
  | "hydrate"
  | "missedProblemsRef"
  | "observeActiveTurn"
  | "refreshActiveTurn"
  | "refreshSessionRecord"
  | "sessionRef"
  | "setActive"
  | "setDraft"
  | "setError"
  | "setFileChanges"
  | "setModels"
  | "setNotice"
  | "setPendingUser"
  | "setRequests"
  | "setStream"
  | "setSubmissionOwner"
  | "setSubmitting"
  | "setWarnings"
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
export function useConversationTransitions({
  activeRef,
  activeTurn,
  clearStream,
  completedTurnsRef,
  draftEditRevision,
  draftOwedRef,
  hydrate,
  missedProblemsRef,
  observeActiveTurn,
  persistDraft,
  refreshActiveTurn,
  refreshSessionRecord,
  sessionRef,
  setActive,
  setDraft,
  setError,
  setFileChanges,
  setModels,
  setNotice,
  setPendingUser,
  setRequests,
  setStream,
  setSubmissionOwner,
  setSubmitting,
  setWarnings,
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
    (
      owner: PendingSubmission,
      status: string,
      reason: string | null | undefined,
      editRevision: number,
    ) => {
      if (owner.turnId) return;
      const selected = sessionRef.current === owner.sessionId;
      if (selected && draftEditRevision.current !== editRevision) {
        setError(
          `${endedMessage(status, reason)} Its message is not in the conversation.`,
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
      if (selected) setDraft(owner.text);
      reportProblem(
        { missedProblemsRef, sessionRef, setError },
        owner.sessionId,
        `${endedMessage(status, reason)} Its message was restored to this conversation.`,
      );
    },
    [
      missedProblemsRef,
      sessionRef,
      draftEditRevision,
      setError,
      persistDraft,
      setDraft,
    ],
  );

  const finishSubmission = useCallback(
    (status: string, reason?: string | null) => {
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
        else if (status !== "completed") setError(endedMessage(status, reason));
      } else if (
        owner.turnId &&
        !["completed", "interrupted"].includes(status)
      ) {
        // An accepted message stays in the transcript, so nothing is restored
        // and this is the only word the researcher gets.
        reportProblem(
          { missedProblemsRef, sessionRef, setError },
          owner.sessionId,
          endedMessage(status, reason),
        );
      }
      void settleTurn(owner.sessionId)
        .then(() => {
          if (status !== "completed")
            restorePrompt(owner, status, reason, editRevision);
        })
        .catch(() => undefined);
    },
    [
      submissionRef,
      submittingRef,
      setSubmissionOwner,
      missedProblemsRef,
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
    missedProblemsRef,
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

  const observeEvent = useCallback(
    ({ event: payload }: DecodedConversationEvent, disposed: () => boolean) => {
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
        completedTurnsRef.current.set(terminalKey(payload.epoch, "", ""), {
          status: "connection closed",
          reason: null,
        });
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
        const selected = sessionRef.current;
        const threadId = String(payload.threadId),
          turnId = payload.turnId;
        setPendingUser(null);
        clearStream();
        void refreshActiveTurn().then((host) => {
          if (
            disposed() ||
            sessionRef.current !== selected ||
            !host ||
            host.automated ||
            host.threadId !== threadId ||
            host.turnId !== turnId ||
            completedTurnsRef.current.has(
              terminalKey(payload.epoch, threadId, turnId),
            )
          )
            return;
          const turn = { threadId, turnId };
          activeRef.current = turn;
          setActive(turn);
        });
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
        payload.kind === "protocolWarning" &&
        typeof payload.message === "string"
      ) {
        reportConnectionWarning(
          { missedProblemsRef, sessionRef, setError, setWarnings },
          owner?.sessionId ?? (activeRef.current ? sessionRef.current : null),
          payload.message,
        );
      }

      if (
        (payload.kind === "itemStarted" || payload.kind === "itemCompleted") &&
        payload.itemKind === "fileChange" &&
        typeof payload.itemId === "string" &&
        payload.item &&
        typeof payload.item === "object"
      ) {
        const { itemId, item } = payload;
        setFileChanges((old) => {
          const changes = new Map(old);
          changes.delete(itemId);
          changes.set(itemId, item as Record<string, unknown>);
          if (changes.size > FILE_CHANGE_LIMIT)
            changes.delete(changes.keys().next().value!);
          return changes;
        });
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
        const status = String(payload.status ?? "failed");
        const reason = turnFailureReason(
          (payload.turn as { error?: unknown } | undefined)?.error,
        );
        completedTurnsRef.current.set(key, { status, reason });
        if (completedTurnsRef.current.size > 100) {
          const oldest = completedTurnsRef.current.keys().next().value;
          if (oldest) completedTurnsRef.current.delete(oldest);
        }
        if (ownsTurn) finishSubmission(status, reason);
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
    },
    [
      finishSubmission,
      activeRef,
      clearStream,
      completedTurnsRef,
      flushStream,
      hydrate,
      missedProblemsRef,
      observeActiveTurn,
      observeAdoptedSend,
      refreshActiveTurn,
      refreshSessionRecord,
      sessionRef,
      setActive,
      setError,
      setFileChanges,
      setPendingUser,
      setRequests,
      setStream,
      setWarnings,
      settleTurn,
      snapshotRef,
      streamBufferRef,
      streamFlushRef,
      streamItemRef,
      streamStore,
      submissionRef,
      turnTextRef,
    ],
  );

  const disposeEvents = useCallback(() => {
    if (streamFlushRef.current !== null)
      window.clearTimeout(streamFlushRef.current);
    streamFlushRef.current = null;
    streamBufferRef.current = "";
    streamItemRef.current = null;
  }, [streamFlushRef, streamBufferRef, streamItemRef]);
  return {
    finishSubmission,
    observeEvent,
    onSubscribed: refreshActiveTurn,
    disposeEvents,
  };
}
