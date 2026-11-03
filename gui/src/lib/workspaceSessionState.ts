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

/**
 * Lands a conversation read on the snapshot it replaces. Only these reads carry
 * a transcript, so a read's items and turns are always the newest there are. A
 * session save that lands during the read raises the revision and sequence on
 * screen, which says only that the session fields are newer: those stay.
 */
export function keepNewerSession(
  current: ConversationSnapshot | null,
  next: ConversationSnapshot,
): ConversationSnapshot {
  if (current?.session.id !== next.session.id) return next;
  const sequence = Math.max(current.sequence, next.sequence);
  return current.session.revision > next.session.revision
    ? {
        ...next,
        session: current.session,
        workspace: current.workspace,
        sequence,
      }
    : { ...next, sequence };
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
