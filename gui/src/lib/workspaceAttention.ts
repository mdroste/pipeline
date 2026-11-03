import type { WorkbenchEvent } from "./workbenchTypes";
/**
 * Unresolved requests by id, with the conversation each one waits in. Returns
 * `current` itself when the event changes nothing, because this runs on every
 * streamed token and a new map would re-render the app shell each time.
 */
export function pendingRequestSessions(
  current: ReadonlyMap<string, string | null>,
  event: WorkbenchEvent,
): ReadonlyMap<string, string | null> {
  if (event.kind === "connectionClosed")
    return current.size ? new Map() : current;
  if (event.requestId === undefined || event.requestId === null) return current;
  const id = JSON.stringify(event.requestId);
  if (event.kind === "serverRequest") {
    const session = event.sessionId ?? null;
    if (current.has(id) && current.get(id) === session) return current;
    return new Map(current).set(id, session);
  }
  if (event.kind === "serverRequestResolved" && current.has(id)) {
    const next = new Map(current);
    next.delete(id);
    return next;
  }
  return current;
}
