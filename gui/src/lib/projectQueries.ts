/** Project reads only. Folder scans remain explicit mutation operations.
 * Concurrent home readers share one request; no settled or cross-project cache. */
import { projectClient } from "./projectClient";
import { workspaceClient } from "./workbench/workspaceClient";
import type { WorkbenchEvent } from "./workbench/runtimeTypes";
const homeReads = new Map<string, ReturnType<typeof fetchHome>>();
async function fetchHome(workspaceId: string) {
  const [home, workspace] = await Promise.all([
    projectClient.home(workspaceId),
    workspaceClient.getWorkspace(workspaceId),
  ]);
  return { home, workspace };
}
export function readProjectHome(workspaceId: string) {
  const current = homeReads.get(workspaceId);
  if (current) return current;
  const request = fetchHome(workspaceId).finally(() => {
    if (homeReads.get(workspaceId) === request) homeReads.delete(workspaceId);
  });
  homeReads.set(workspaceId, request);
  return request;
}
/** Older hosts omit project identity; retain their conservative refresh behavior. */
export function invalidatesProjectHome(
  event: WorkbenchEvent,
  workspaceId: string,
) {
  return (
    event.kind === "turnCompleted" &&
    (typeof event.workspaceId !== "string" || event.workspaceId === workspaceId)
  );
}
