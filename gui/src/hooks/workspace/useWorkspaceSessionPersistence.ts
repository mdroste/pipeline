import { useCallback, useEffect, useRef } from "react";
import { conversationClient } from "../../lib/workbench/conversationClient";
import {
  mergeSessionSnapshot,
  newOperation,
  saveWorkspaceDraft,
} from "../../lib/workspaceSessionState";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type { WorkspaceConversationRuntime } from "./useWorkspaceConversationRuntime";

type PersistenceContext = Pick<
  WorkspaceConversationRuntime,
  | "sessionId"
  | "runtime"
  | "sessionRef"
  | "snapshotRef"
  | "activeRef"
  | "submittingRef"
  | "draftRef"
  | "draftEditRevision"
  | "setDraft"
  | "setSnapshot"
  | "setError"
>;
/** Drafts and model selections share this one revision-ordered write lane. */
export function useWorkspaceSessionPersistence({
  sessionId,
  runtime,
  sessionRef,
  snapshotRef,
  activeRef,
  submittingRef,
  draftRef,
  draftEditRevision,
  setDraft,
  setSnapshot,
  setError,
}: PersistenceContext) {
  const selectionSaveRef = useRef<Promise<void>>(Promise.resolve());
  const sessionWriteRef = useRef<Promise<unknown>>(Promise.resolve());
  const draftSaveRef = useRef<Promise<unknown>>(Promise.resolve());
  const persistDraft = useCallback((selectedSession: string, text: string) => {
    // Navigation can flush a draft while its debounce save is in flight.
    const pending = sessionWriteRef.current
      .catch(() => undefined)
      .then(() => saveWorkspaceDraft(selectedSession, text))
      .then((next) => {
        if (sessionRef.current === selectedSession)
          setSnapshot((old) => mergeSessionSnapshot(old, next));
        return next;
      });
    draftSaveRef.current = pending;
    sessionWriteRef.current = pending;
    return pending;
  }, []);

  const editDraft = (text: string) => {
    draftEditRevision.current += 1;
    if (sessionId)
      try {
        localStorage.setItem(`pipeline.pendingDraft.${sessionId}`, text);
      } catch {
        /* Server debounce remains available. */
      }
    setDraft(text);
  };
  useEffect(
    () => () => {
      const id = sessionRef.current;
      if (
        id &&
        snapshotRef.current?.session.id === id &&
        !activeRef.current &&
        !submittingRef.current
      )
        void persistDraft(id, draftRef.current).catch(() => undefined);
    },
    [persistDraft],
  );
  const saveCurrentDraft = useCallback(async () => {
    await selectionSaveRef.current;
    const id = sessionRef.current;
    if (
      id &&
      snapshotRef.current?.session.id === id &&
      !activeRef.current &&
      !submittingRef.current
    )
      await persistDraft(id, draftRef.current);
  }, [persistDraft]);

  const saveSelection = (
    nextModel: string,
    nextEffort: string,
    nextRuntime = runtime,
  ) => {
    const selectedSession = sessionRef.current;
    if (!selectedSession) return;
    const pending = sessionWriteRef.current
      .catch(() => undefined)
      .then(async () => {
        const latest =
          await conversationClient.sessionSnapshot(selectedSession);
        const updated = await conversationClient.updateSession({
          sessionId: latest.session.id,
          expectedRevision: latest.session.revision,
          operationId: newOperation("model-selection"),
          overrides: {
            ...latest.session.overrides,
            runtime: nextRuntime,
            model: nextModel || null,
            effort: nextEffort || null,
          },
        });
        if (sessionRef.current === selectedSession) {
          setSnapshot((old) =>
            mergeSessionSnapshot(old, {
              ...latest,
              session: updated.record,
              sequence: updated.sequence,
            }),
          );
        }
      });
    sessionWriteRef.current = pending;
    selectionSaveRef.current = pending;
    void pending.catch((cause) => {
      if (sessionRef.current === selectedSession)
        setError(
          `Model selection could not be saved: ${workbenchErrorMessage(cause)}. Select the model again to retry.`,
        );
    });
  };

  return {
    persistDraft,
    editDraft,
    saveCurrentDraft,
    saveSelection,
    selectionSaveRef,
    draftSaveRef,
  };
}
export type WorkspaceSessionPersistence = ReturnType<
  typeof useWorkspaceSessionPersistence
>;

/** Registered after selection reconciliation, before the native event bridge. */
export function useWorkspaceDraftAutosave(
  conversation: WorkspaceConversationRuntime,
  persistence: WorkspaceSessionPersistence,
  contextBusy: boolean,
) {
  const {
    active,
    draft,
    pendingUser,
    sessionId,
    snapshot,
    submitting,
    sessionRef,
    setSnapshot,
    setError,
  } = conversation;
  const { persistDraft } = persistence;
  useEffect(() => {
    const current = snapshot;
    if (
      !current ||
      current.session.id !== sessionId ||
      pendingUser ||
      active ||
      submitting ||
      contextBusy ||
      current.session.draft === draft
    )
      return;
    const selectedSession = current.session.id;
    const text = draft;
    const timer = window.setTimeout(() => {
      void persistDraft(selectedSession, text)
        .then((next) => {
          if (sessionRef.current === selectedSession)
            setSnapshot((old) => mergeSessionSnapshot(old, next));
        })
        .catch((cause) =>
          setError(`Draft could not be saved: ${workbenchErrorMessage(cause)}`),
        );
    }, 350);
    return () => window.clearTimeout(timer);
  }, [
    active,
    contextBusy,
    draft,
    pendingUser,
    persistDraft,
    sessionId,
    snapshot,
    submitting,
  ]);
}
