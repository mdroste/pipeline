import { type Dispatch, type SetStateAction, type RefObject } from "react";
import { runtimeClient } from "../../lib/workbench/runtimeClient";
import { newOperation } from "../../lib/workspaceSessionState";
import type { WorkspaceSessionPersistence } from "./useWorkspaceSessionPersistence";
import type { SubmissionIntents } from "./submissionIntents";
interface SubmissionPresentation {
  contextBusyRef: RefObject<boolean>;
  followLatestRef: RefObject<boolean>;
  setSelectedMessage: Dispatch<SetStateAction<string | null>>;
  setJumpTarget: Dispatch<SetStateAction<string | null>>;
  setTranscriptPage: Dispatch<SetStateAction<number>>;
}
/** Transport ordering: claim, await persistence barriers, dispatch, acknowledge. */
export function createWorkspaceSubmission(
  intents: SubmissionIntents,
  persistence: Pick<
    WorkspaceSessionPersistence,
    "selectionSaveRef" | "draftSaveRef" | "editDraft" | "persistDraft"
  >,
  finishSubmission: (status: string, reason?: string | null) => void,
  presentation: SubmissionPresentation,
) {
  return async () => {
    const ticket = intents.begin(presentation.contextBusyRef.current);
    if (!ticket) return;
    presentation.followLatestRef.current = true;
    presentation.setSelectedMessage(null);
    presentation.setJumpTarget(null);
    presentation.setTranscriptPage(0);
    let dispatched = false;
    try {
      await persistence.selectionSaveRef.current;
      await persistence.draftSaveRef.current.catch(() => undefined);
      dispatched = true;
      const result = await runtimeClient.sendTurn({
        sessionId: ticket.sessionId,
        text: ticket.text,
        clientSubmissionId: newOperation("submission"),
        model: ticket.model || null,
        effort: ticket.effort || null,
      });
      await intents.acknowledge(ticket, result, persistence, finishSubmission);
    } catch (cause) {
      await intents.fail(ticket, cause, dispatched, persistence);
    }
  };
}
