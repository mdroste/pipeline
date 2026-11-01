import { useCallback, useEffect, useRef } from "react";
import { conversationClient } from "../../lib/workbench/conversationClient";
import {
  mergeSessionSnapshot,
  newOperation,
  saveWorkspaceDraft,
} from "../../lib/workspaceSessionState";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { conversationProvider } from "../../lib/workspaceProviders";
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
  | "draftOwedRef"
  | "setDraft"
  | "setSnapshot"
  | "setError"
  | "setModel"
  | "setEffort"
  | "setRuntime"
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
  draftOwedRef,
  setDraft,
  setSnapshot,
  setError,
  setModel,
  setEffort,
  setRuntime,
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
        draftOwedRef.current !== id &&
        !activeRef.current &&
        !submittingRef.current
      )
        void persistDraft(id, draftRef.current).catch(() => undefined);
    },
    [persistDraft],
  );
  // Only sending depends on a selection save. Leaving must still work when
  // one failed, or that one failure would hold the researcher on this page.
  const saveCurrentDraft = useCallback(async () => {
    await selectionSaveRef.current.catch(() => undefined);
    const id = sessionRef.current;
    if (
      id &&
      snapshotRef.current?.session.id === id &&
      draftOwedRef.current !== id &&
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
      if (sessionRef.current !== selectedSession) return;
      setError(
        `Model selection could not be saved: ${workbenchErrorMessage(cause)}. Select the model again to retry.`,
      );
      // Show what is saved, so selecting the same model again counts as a
      // change. A newer selection speaks for itself.
      const saved = snapshotRef.current;
      if (selectionSaveRef.current !== pending || !saved) return;
      if (saved.session.id !== selectedSession) return;
      const { overrides } = saved.session;
      setRuntime(conversationProvider(overrides.runtime));
      setModel(typeof overrides.model === "string" ? overrides.model : "");
      setEffort(typeof overrides.effort === "string" ? overrides.effort : "");
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
      void persistDraft(selectedSession, text).catch((cause) =>
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
