/** Submission transitions are owned by the conversation runtime. The transport
 * adapter receives a ticket and intents, never mutable model refs. */
import { conversationClient } from "../../lib/workbench/conversationClient";
import { runtimeClient } from "../../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { reportProblem } from "./reportProblem";
import { busyMessage, terminalKey } from "./useAdoptedSubmission";
import type { WorkspaceSessionPersistence } from "./useWorkspaceSessionPersistence";
import type { WorkspaceConversationState } from "./useWorkspaceConversationRuntime";
import type { PendingSubmission } from "./conversationRuntimeTypes";

import type { SendTurnResult } from "../../lib/workbench/runtimeTypes";
type SubmissionRuntime = Pick<
  WorkspaceConversationState,
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

type Persistence = Pick<
  WorkspaceSessionPersistence,
  "editDraft" | "persistDraft"
>;
export function createSubmissionIntents(conversation: SubmissionRuntime) {
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
  return {
    begin(contextBusy: boolean) {
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
      if (active || submittingRef.current || running || contextBusy) return;
      hydrationGeneration.current += 1;
      submittingRef.current = true;
      setSubmitting(true);
      const text = draft.trim();
      const submission: PendingSubmission = { sessionId, text };
      submissionRef.current = submission;
      setSubmissionOwner({ sessionId, threadId: null });
      clearStream();
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
      return { sessionId, text, submission, editRevisionAtSend, model, effort };
    },
    async acknowledge(
      ticket: SubmissionTicket,
      result: SendTurnResult,
      { persistDraft }: Persistence,
      finishSubmission: (status: string, reason?: string | null) => void,
    ) {
      const { sessionId, text, submission, editRevisionAtSend } = ticket;
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
    },
    async fail(
      ticket: SubmissionTicket,
      cause: unknown,
      dispatched: boolean,
      { editDraft }: Persistence,
    ) {
      const { sessionId, text, editRevisionAtSend } = ticket;
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
    },
  };
}
export interface SubmissionTicket {
  sessionId: string;
  text: string;
  submission: PendingSubmission;
  editRevisionAtSend: number;
  model: string;
  effort: string;
}
export type SubmissionIntents = ReturnType<typeof createSubmissionIntents>;
