import type { WorkbenchEvent } from "./workbenchTypes";
export function pendingRequestIds(
  current: ReadonlySet<string>,
  event: WorkbenchEvent,
): Set<string> {
  const next = new Set(current);
  if (event.kind === "connectionClosed") return new Set();
  if (event.requestId === undefined || event.requestId === null) return next;
  const id = JSON.stringify(event.requestId);
  if (event.kind === "serverRequest") next.add(id);
  if (event.kind === "serverRequestResolved") next.delete(id);
  return next;
}
