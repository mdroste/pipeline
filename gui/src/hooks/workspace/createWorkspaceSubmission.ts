import { conversationClient } from "../../lib/workbench/conversationClient";
import { type Dispatch, type SetStateAction, type RefObject } from "react";
import { runtimeClient } from "../../lib/workbench/runtimeClient";
import { newOperation } from "../../lib/workspaceSessionState";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { reportProblem } from "./reportProblem";
import { busyMessage, terminalKey } from "./useAdoptedSubmission";
import type { WorkspaceSessionPersistence } from "./useWorkspaceSessionPersistence";
import type { WorkspaceConversationRuntime } from "./useWorkspaceConversationRuntime";
import type { PendingSubmission } from "./conversationRuntimeTypes";

type SubmissionRuntime = Pick<
  WorkspaceConversationRuntime,
  | "hydrationGeneration"
  | "sessionId"
  | "snapshotRef"
  | "draft"
  | "active"
  | "submittingRef"
  | "submissionRef"
  | "setSubmitting"
  | "setSubmissionOwner"
  | "clearStream"
  | "setError"
  | "setNotice"
  | "setWarnings"
  | "setPendingUser"
  | "setDraft"
  | "draftEditRevision"
  | "model"
  | "effort"
  | "completedTurnsRef"
  | "sessionRef"
  | "activeRef"
  | "setActive"
  | "setRequests"
  | "hydrate"
  | "refreshActiveTurn"
  | "missedProblemsRef"
>;

interface SubmissionPresentation {
  contextBusyRef: RefObject<boolean>;
  followLatestRef: RefObject<boolean>;
  setSelectedMessage: Dispatch<SetStateAction<string | null>>;
  setJumpTarget: Dispatch<SetStateAction<string | null>>;
  setTranscriptPage: Dispatch<SetStateAction<number>>;
}
/** Create the send operation using the same identity and the shared persistence barriers. */
export function createWorkspaceSubmission(
  conversation: SubmissionRuntime,
  persistence: Pick<
    WorkspaceSessionPersistence,
    "selectionSaveRef" | "draftSaveRef" | "editDraft" | "persistDraft"
  >,
  finishSubmission: (status: string, reason?: string | null) => void,
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
    setWarnings,
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
    setWarnings([]);
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
      // The host saves the draft with the message, so a failed autosave is not
      // a reason to hold this send back.
      await draftSaveRef.current.catch(() => undefined);
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
        void persistDraft(sessionId, "", text)
          .catch(() => conversationClient.sessionSnapshot(sessionId))
          .then((latest) => {
            if (
              sessionRef.current === sessionId &&
              draftEditRevision.current === editRevisionAtSend
            )
              setDraft(latest.session.draft);
          })
          .catch((error) => setError(workbenchErrorMessage(error)));
      const key = terminalKey(result.epoch, result.threadId, result.turnId);
      const terminal =
        completedTurnsRef.current.get(key) ??
        completedTurnsRef.current.get(terminalKey(result.epoch, "", ""));
      if (terminal) {
        completedTurnsRef.current.delete(key);
        finishSubmission(terminal.status, terminal.reason);
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
        reportProblem(conversation, sessionId, message);
        return;
      }
      if (sessionRef.current === sessionId) setPendingUser(null);
      else {
        try {
          localStorage.setItem(`pipeline.pendingDraft.${sessionId}`, text);
        } catch {
          /* The host keeps the saved draft until it accepts a message. */
        }
      }
      reportProblem(
        conversation,
        sessionId,
        `Pipeline could not confirm whether the message was received. It was preserved as a draft and will not be retried automatically. ${workbenchErrorMessage(cause)}`,
      );
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
