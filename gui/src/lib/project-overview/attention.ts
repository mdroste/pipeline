import type { OverviewRow } from "./types";

export type AttentionPreferences = Record<
  string,
  {
    version: string;
    pinned?: boolean;
    pinnedUntil?: number;
    laterUntil?: number;
  }
>;
const key = (workspaceId: string) =>
  `pipeline.project.attention.${workspaceId}`;
export const attentionVersion = (row: OverviewRow) =>
  row.version ?? `${row.text}\n${row.detail ?? ""}`;
export function loadAttentionPreferences(
  workspaceId: string,
): AttentionPreferences {
  try {
    const value = JSON.parse(localStorage.getItem(key(workspaceId)) ?? "{}");
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    return Object.fromEntries(
      Object.entries(value).filter(([, entry]) => {
        if (!entry || typeof entry !== "object") return false;
        const v = entry as Record<string, unknown>;
        return (
          typeof v.version === "string" &&
          (v.laterUntil === undefined || typeof v.laterUntil === "number")
        );
      }),
    ) as AttentionPreferences;
  } catch {
    return {};
  }
}
export function saveAttentionPreferences(
  workspaceId: string,
  value: AttentionPreferences,
) {
  localStorage.setItem(key(workspaceId), JSON.stringify(value));
}
export function deferred(
  row: OverviewRow,
  prefs: AttentionPreferences,
  now = Date.now(),
) {
  const preference = prefs[row.id];
  return (
    row.category !== "blocker" &&
    preference?.version === attentionVersion(row) &&
    (preference.laterUntil ?? 0) > now
  );
}
export function orderAttention(
  rows: OverviewRow[],
  prefs: AttentionPreferences,
) {
  return [...rows].sort((a, b) => {
    const rank = (row: OverviewRow) =>
      row.category === "blocker" ? 0 : priorityPinned(row, prefs) ? 1 : 2;
    return rank(a) - rank(b);
  });
}
export const priorityPinned = (row: OverviewRow, prefs: AttentionPreferences) =>
  prefs[row.id]?.version === attentionVersion(row) &&
  Boolean(prefs[row.id]?.pinned) &&
  (prefs[row.id]?.pinnedUntil ?? Infinity) > Date.now();
