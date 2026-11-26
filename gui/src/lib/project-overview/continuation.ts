import type { FileLocation } from "../fileLinks";
import type { WorkspaceDestination } from "../workspaceNavigation";
import { isWorkspaceDestination } from "../workspaceNavigation";

export interface RecentWork {
  id: string;
  title: string;
  at: string;
  tab: WorkspaceDestination;
  revisionId?: string;
  location?: FileLocation;
  targetId?: string;
}
const key = (id: string) => `pipeline.project.recent-work.${id}`;
export function recentWork(workspaceId: string): RecentWork[] {
  try {
    const value: unknown = JSON.parse(
      localStorage.getItem(key(workspaceId)) ?? "[]",
    );
    if (!Array.isArray(value)) return [];
    return value
      .filter(
        (item): item is RecentWork =>
          item &&
          typeof item.id === "string" &&
          typeof item.title === "string" &&
          typeof item.at === "string" &&
          isWorkspaceDestination(item.tab) &&
          (!item.location ||
            (typeof item.location.path === "string" &&
              !item.location.path.includes("\0"))) &&
          (!item.revisionId || typeof item.revisionId === "string") &&
          (!item.targetId || typeof item.targetId === "string"),
      )
      .slice(0, 6);
  } catch {
    return [];
  }
}
export function rememberWork(
  workspaceId: string,
  work: Omit<RecentWork, "at">,
) {
  const old = recentWork(workspaceId);
  if (
    old[0]?.id === work.id &&
    JSON.stringify(old[0].location) === JSON.stringify(work.location) &&
    Date.now() - Date.parse(old[0].at) < 5000
  )
    return;
  try {
    localStorage.setItem(
      key(workspaceId),
      JSON.stringify(
        [
          { ...work, at: new Date().toISOString() },
          ...old.filter((item) => item.id !== work.id),
        ].slice(0, 6),
      ),
    );
  } catch {
    /* Navigation history must never interrupt editing. */
  }
}
