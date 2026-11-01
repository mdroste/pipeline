import { useCallback, useEffect, useRef } from "react";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type { ActiveTurn, WorkbenchEvent } from "../../lib/workbenchTypes";
import type { WorkspaceConversationRuntime } from "./useWorkspaceConversationRuntime";

export const terminalKey = (epoch: unknown, thread: unknown, turn: unknown) =>
  `${String(epoch)}:${String(thread)}:${String(turn)}`;

/** The host refused a send because a turn held the permit. Names that turn. */
export function busyMessage(
  turn: ActiveTurn | null | undefined,
  sessionId: string,
) {
  if (!turn)
    return "Message not sent: Pipeline was busy for a moment. Send it again.";
  const here = turn.sessionId === sessionId;
  const where = here
    ? "this conversation"
    : turn.title
      ? `“${turn.title}”`
      : "another conversation";
  if (turn.automated)
    return `Message not sent: an automation is running in ${where}. Send it when that finishes, or open Automations to stop it.`;
  return here
    ? "Message not sent: a response is still running in this conversation. Send it when that response finishes."
    : `Message not sent: a response is still running in ${where}. Send it when that response finishes, or open that conversation to stop it.`;
}

/** Reads of the saved draft to try before leaving the composer locked. */
const DRAFT_RELOAD_ATTEMPTS = 3;
/** Shown when a send ended without a turn and its saved draft can't be read. */
const DRAFT_UNAVAILABLE =
  "This conversation could not be reloaded, so the message box stays locked. Your unsent message is safe. Open the conversation again to continue.";

/** A send that failed after its page closed, in the words its sender would have seen. */
function adoptedFailureMessage(failure: { code: string; message: string }) {
  return failure.code === "turn_not_sent"
    ? `Message not sent: ${workbenchErrorMessage(failure)}`
    : `Pipeline could not confirm whether the message was received. It was preserved as a draft and will not be retried automatically. ${workbenchErrorMessage(failure)}`;
}

type AdoptionContext = Pick<
  WorkspaceConversationRuntime,
  | "activeRef"
  | "activeTurn"
  | "completedTurnsRef"
  | "draftOwedRef"
  | "hydrate"
  | "sessionRef"
  | "setActive"
  | "setError"
  | "setPendingUser"
  | "setSubmissionOwner"
  | "setSubmitting"
  | "submissionOwner"
  | "submissionRef"
  | "submittingRef"
> & { finishSubmission: (status: string) => void };

/**
 * Follows a send found running after its page closed. It has no promise to
 * wait on, so the host's running turn says how its setup ended. The host settles
 * the saved draft before it announces the end of setup, so the reloaded draft is
 * the message only if it wasn't delivered. Returns the event observer that
 * records why a send failed.
 */
export function useAdoptedSubmission({
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
}: AdoptionContext) {
  const failureRef = useRef<{ code: string; message: string } | null>(null);
  useEffect(() => {
    const owner = submissionRef.current;
    if (!owner?.adopted || activeTurn === undefined) return;
    const turn = activeTurn?.sessionId === owner.sessionId ? activeTurn : null;
    const selected = sessionRef.current === owner.sessionId;
    if (turn?.threadId && turn.turnId) {
      const { threadId, turnId, epoch } = turn;
      Object.assign(owner, {
        threadId,
        turnId,
        epoch: epoch ?? undefined,
        adopted: false,
      });
      submittingRef.current = false;
      setSubmissionOwner({ sessionId: owner.sessionId, threadId });
      const key = terminalKey(epoch, threadId, turnId);
      const terminal = completedTurnsRef.current.get(key);
      if (terminal) {
        completedTurnsRef.current.delete(key);
        finishSubmission(terminal);
      } else if (selected) {
        activeRef.current = { threadId, turnId };
        setActive({ threadId, turnId });
        setSubmitting(false);
      }
    } else if (!turn) {
      const failure = failureRef.current;
      failureRef.current = null;
      submissionRef.current = null;
      submittingRef.current = false;
      setSubmissionOwner(null);
      if (!selected) return;
      // Hydration unlocks the composer in the render that returns its draft.
      // Unlocking first would let autosave write the empty composer over it,
      // so a read that fails or loses to another leaves the composer locked.
      void (async () => {
        const owed = () => draftOwedRef.current === owner.sessionId;
        for (let tries = 0; tries < DRAFT_RELOAD_ATTEMPTS; tries += 1) {
          await hydrate(owner.sessionId, true);
          if (!owed() || sessionRef.current !== owner.sessionId) break;
        }
        if (sessionRef.current !== owner.sessionId) return;
        const message = failure ? adoptedFailureMessage(failure) : null;
        if (owed()) {
          setError(
            message ? `${message} ${DRAFT_UNAVAILABLE}` : DRAFT_UNAVAILABLE,
          );
          return;
        }
        if (!submissionRef.current) {
          setSubmitting(false);
          setPendingUser(null);
        }
        if (message) setError(message);
      })();
    }
  }, [
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
    // Adoption itself must run this check against the latest answer.
    submissionOwner,
    submissionRef,
    submittingRef,
  ]);
  return useCallback(
    (event: WorkbenchEvent) => {
      const owner = submissionRef.current;
      if (
        event.kind === "activeTurnChanged" &&
        event.sendError &&
        owner?.adopted &&
        event.sessionId === owner.sessionId
      )
        failureRef.current = event.sendError;
    },
    [submissionRef],
  );
}
