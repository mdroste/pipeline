import { conversationClient } from "./workbench/conversationClient";
import type { ConversationSnapshot, SessionSnapshot } from "./workbenchTypes";

// Metadata saves must not replace transcript pages or active-turn state.
export function mergeSessionSnapshot(
  current: ConversationSnapshot | null,
  next: SessionSnapshot,
): ConversationSnapshot | null {
  if (
    !current ||
    current.session.id !== next.session.id ||
    current.session.revision > next.session.revision
  )
    return current;
  return {
    ...current,
    session: next.session,
    workspace: next.workspace,
    sequence: Math.max(current.sequence, next.sequence),
  };
}

// The caller serializes this write with other session mutations.
export function newOperation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

export async function saveWorkspaceDraft(
  selectedSession: string,
  text: string,
): Promise<SessionSnapshot> {
  const latest = await conversationClient.sessionSnapshot(selectedSession);
  let result = latest;
  if (latest.session.draft !== text) {
    const updated = await conversationClient.updateSession({
      sessionId: selectedSession,
      expectedRevision: latest.session.revision,
      operationId: `save-draft-${crypto.randomUUID()}`,
      draft: text,
    });
    result = { ...latest, session: updated.record, sequence: updated.sequence };
  }
  try {
    if (
      localStorage.getItem(`pipeline.pendingDraft.${selectedSession}`) === text
    )
      localStorage.removeItem(`pipeline.pendingDraft.${selectedSession}`);
  } catch {
    /* The server save remains authoritative. */
  }
  return result;
}
