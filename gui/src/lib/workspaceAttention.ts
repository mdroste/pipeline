import type { WorkbenchEvent } from "./workbenchTypes";
/** Unresolved requests by id, with the conversation each one waits in. */
export function pendingRequestSessions(
  current: ReadonlyMap<string, string | null>,
  event: WorkbenchEvent,
): Map<string, string | null> {
  const next = new Map(current);
  if (event.kind === "connectionClosed") return new Map();
  if (event.requestId === undefined || event.requestId === null) return next;
  const id = JSON.stringify(event.requestId);
  if (event.kind === "serverRequest") next.set(id, event.sessionId ?? null);
  if (event.kind === "serverRequestResolved") next.delete(id);
  return next;
}
