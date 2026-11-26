/** One draft per adapter scope and path, shared by Files and Write. Saved captures
 * and isolated edit copies have different adapter IDs and never share a draft. */
import { movedPath } from "./fileTree";
export interface SharedFileDraft {
  file: { path: string; hash: string; text: string };
  draft: string;
}
type Listener = (value: SharedFileDraft, origin?: object) => void;
const listeners = new Map<string, Set<Listener>>();
const published = new Map<string, string>();
const unpersisted = new Map<string, SharedFileDraft>();
export const fileDraftKey = (adapterId: string, path: string) =>
  `pipeline.files.draft.${adapterId}.${path}`;
export const projectFileScope = (
  workspaceId: string,
  checkpoint?: string | null,
) => `workspace:${workspaceId}:${checkpoint || "working"}`;

/** Include drafts retained only in memory after storage filled up. */
export function listFileDraftPaths(adapterId: string): string[] {
  const prefix = fileDraftKey(adapterId, "");
  const keys = new Set(unpersisted.keys());
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key) keys.add(key);
  }
  return [...keys].flatMap((key) => {
    if (!key.startsWith(prefix)) return [];
    const path = key.slice(prefix.length);
    // Scope metadata disambiguates prefixes when present. Older/corrupt draft
    // envelopes still count for destructive-operation warnings; opening validates them.
    try {
      const value =
        unpersisted.get(key) ?? JSON.parse(localStorage.getItem(key) ?? "null");
      return typeof value?.file?.path === "string" && value.file.path !== path
        ? []
        : [path];
    } catch {
      return [path];
    }
  });
}

export function removeFileDraft(adapterId: string, path: string) {
  const key = fileDraftKey(adapterId, path);
  localStorage.removeItem(key);
  unpersisted.delete(key);
  published.delete(key);
}

/** Persist the new identity before removing the old one. A failed move retains
 * the source, so callers can still recover it when storage is unavailable. */
export function moveFileDrafts(adapterId: string, from: string, to: string) {
  for (const path of listFileDraftPaths(adapterId)) {
    const target = movedPath(path, from, to);
    if (!target) continue;
    const key = fileDraftKey(adapterId, path);
    let retained;
    try {
      retained =
        unpersisted.get(key) ?? JSON.parse(localStorage.getItem(key) ?? "null");
    } catch {
      continue; // One corrupt envelope must not prevent other drafts moving.
    }
    if (retained?.file?.path !== path) continue;
    // Moving preserves historical/partial draft envelopes too. Validation belongs
    // to the editor that opens one, not to a filename change.
    localStorage.setItem(
      fileDraftKey(adapterId, target),
      JSON.stringify({
        ...retained,
        file: { ...retained.file, path: target },
      }),
    );
    removeFileDraft(adapterId, path);
  }
}
export function readFileDraft(
  adapterId: string,
  path: string,
): SharedFileDraft | null {
  const key = fileDraftKey(adapterId, path);
  const value =
    unpersisted.get(key) ?? JSON.parse(localStorage.getItem(key) ?? "null");
  return value &&
    value.file?.path === path &&
    typeof value.file.hash === "string" &&
    typeof value.file.text === "string" &&
    typeof value.draft === "string"
    ? value
    : null;
}
export function writeFileDraft(
  adapterId: string,
  value: SharedFileDraft,
  origin?: object,
) {
  const key = fileDraftKey(adapterId, value.file.path);
  const signature = JSON.stringify(value);
  let failure: unknown;
  try {
    if (value.draft === value.file.text) localStorage.removeItem(key);
    else localStorage.setItem(key, signature);
    unpersisted.delete(key);
  } catch (error) {
    unpersisted.set(key, value);
    if (value.draft !== value.file.text) failure = error;
  }
  if (published.get(key) !== signature) {
    published.set(key, signature);
    if (published.size > 64) published.delete(published.keys().next().value!);
    for (const listener of listeners.get(adapterId) ?? [])
      listener(value, origin);
  }
  if (failure) throw failure;
}
export function subscribeFileDrafts(adapterId: string, listener: Listener) {
  const set = listeners.get(adapterId) ?? new Set<Listener>();
  listeners.set(adapterId, set);
  set.add(listener);
  return () => {
    set.delete(listener);
    if (!set.size) listeners.delete(adapterId);
  };
}
