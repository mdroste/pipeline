import {
  useEffect,
  useCallback,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  ConversationSnapshot,
  WorkbenchEvent,
  WorkspaceModel,
} from "../lib/workbenchTypes";

const STREAM_FLUSH_MS = 50;

type ActiveTurn = { threadId: string; turnId: string };
export interface PendingSubmission {
  sessionId: string;
  text: string;
  threadId?: string;
  turnId?: string;
  epoch?: number;
}
export const terminalKey = (epoch: unknown, thread: unknown, turn: unknown) =>
  `${String(epoch)}:${String(thread)}:${String(turn)}`;

interface WorkbenchEventBridge {
  activeRef: MutableRefObject<ActiveTurn | null>;
  completedTurnsRef: MutableRefObject<Map<string, string>>;
  hydrate: (sessionId?: string | null) => Promise<void>;
  submissionRef: MutableRefObject<PendingSubmission | null>;
  persistDraft: (
    sessionId: string,
    draft: string,
  ) => Promise<ConversationSnapshot>;
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
  setTasksEnabled: Dispatch<SetStateAction<boolean>>;
  snapshotRef: MutableRefObject<ConversationSnapshot | null>;
  streamBufferRef: MutableRefObject<string>;
  streamFlushRef: MutableRefObject<number | null>;
  submittingRef: MutableRefObject<boolean>;
}

/** Owns the native runtime event subscription and terminal-turn cleanup. */
export function useWorkbenchEvents({
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
  setTasksEnabled,
  snapshotRef,
  streamBufferRef,
  streamFlushRef,
  submittingRef,
}: WorkbenchEventBridge) {
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
            if (sessionRef.current === owner.sessionId) setSnapshot(next);
          })
          .catch(() => undefined);
        if (selected) {
          setDraft(owner.text);
          setError(
            `The turn ended with status ${status}. Its message was restored to this conversation.`,
          );
        }
      } else {
        void workbenchClient
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
        setRequests((old) =>
          old.filter((request) => request.epoch !== payload.epoch),
        );
      }

      if (
        payload.kind === "accountUpdated" ||
        payload.kind === "accountLoginCompleted"
      ) {
        void workbenchClient
          .accountState(true)
          .then(async (account) => {
            if (account.status === "chatgpt")
              setModels((await workbenchClient.modelCatalog()).models);
            else
              setError(
                "The project assistant is no longer signed in to ChatGPT. Your draft and conversation remain local.",
              );
          })
          .catch((cause) => setError(workbenchErrorMessage(cause)));
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
        String(payload.params?.tool ?? "").startsWith("workbench_task_") &&
        matchesThread(payload.params?.threadId)
      ) {
        localStorage.setItem("pipeline.tasks.enabled", "true");
        setTasksEnabled(true);
      }
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
          activeRef.current?.turnId === payload.turnId
        ) {
          activeRef.current = null;
          setActive(null);
          setStream("");
          const selected = sessionRef.current;
          if (selected)
            void workbenchClient
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
    setTasksEnabled,
    snapshotRef,
    streamBufferRef,
    streamFlushRef,
    submittingRef,
  ]);
  return { finishSubmission };
}
