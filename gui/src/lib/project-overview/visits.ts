import type { FileEntry } from "../projectClient";

// ---- Visits ---------------------------------------------------------------

export type FileSnapshot = Record<string, string>;
export interface Visit {
  /** When the previous visit ended; null on the first visit to a project. */
  since: string | null;
  /** The folder as it was when the previous visit ended. */
  baseline: FileSnapshot | null;
}
interface StoredVisit extends Visit {
  version: 1;
  lastActive: string;
  latest: FileSnapshot | null;
}
/** A gap this long between activity starts a new visit. */
export const VISIT_GAP_MS = 30 * 60_000;
const SNAPSHOT_LIMIT = 3000;
const visitKey = (workspaceId: string) =>
  `pipeline.project.visit.${workspaceId}`;

export function snapshotFiles(
  files: FileEntry[] | null | undefined,
): FileSnapshot | null {
  if (!files || files.length > SNAPSHOT_LIMIT) return null;
  return Object.fromEntries(
    files
      .filter((file) => file.hash)
      .map((file) => [file.path, file.hash!.slice(0, 12)]),
  );
}
export function diffFiles(baseline: FileSnapshot, current: FileSnapshot) {
  const changed: string[] = [];
  const added: string[] = [];
  for (const [path, hash] of Object.entries(current)) {
    if (!(path in baseline)) added.push(path);
    else if (baseline[path] !== hash) changed.push(path);
  }
  const removed = Object.keys(baseline).filter((path) => !(path in current));
  return { changed, added, removed };
}

/**
 * Records activity in a project and reports the visit it belongs to. `since`
 * and `baseline` stay fixed for the whole visit so the overview keeps showing
 * what changed while the researcher was away, not what changed a minute ago.
 */
export function recordVisit(
  workspaceId: string,
  now: Date,
  files: FileEntry[] | null | undefined,
): Visit {
  const snapshot = snapshotFiles(files);
  let stored: StoredVisit | null = null;
  try {
    const raw: unknown = JSON.parse(
      localStorage.getItem(visitKey(workspaceId)) ?? "null",
    );
    if (
      raw &&
      typeof raw === "object" &&
      "version" in raw &&
      raw.version === 1 &&
      "lastActive" in raw &&
      typeof raw.lastActive === "string"
    )
      stored = raw as StoredVisit;
  } catch {
    /* A corrupt marker starts a first visit. */
  }
  const resumed =
    stored !== null &&
    now.getTime() - Date.parse(stored.lastActive) <= VISIT_GAP_MS;
  const visit: Visit = !stored
    ? { since: null, baseline: null }
    : resumed
      ? { since: stored.since, baseline: stored.baseline }
      : { since: stored.lastActive, baseline: stored.latest };
  try {
    localStorage.setItem(
      visitKey(workspaceId),
      JSON.stringify({
        version: 1,
        ...visit,
        lastActive: now.toISOString(),
        latest: snapshot ?? stored?.latest ?? null,
      } satisfies StoredVisit),
    );
  } catch {
    /* Visit markers are a convenience; the overview works without them. */
  }
  return visit;
}
