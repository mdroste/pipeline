// What the Version control page holds, by project, for as long as the app runs.
// This outlives the page on purpose: it unmounts when the project changes tabs, as
// Open in Files does, while a commit message and a running push must both still be
// there on the way back. An action that finishes in the meantime lands here too, so
// its result or error is on the page when the researcher returns.
import { useCallback, useSyncExternalStore } from "react";
import type { VersionControlState } from "../../lib/versionControlClient";

export const NETWORK = { fetch: "Checking", pull: "Pulling", push: "Pushing" };
export type NetworkKind = keyof typeof NETWORK;

export interface PageSession {
  state: VersionControlState | null;
  /** Counts each new `state`, so what depends on it loads again. */
  stamp: number;
  message: { summary: string; description: string };
  /** The merge message last offered in the commit box. */
  offered: string;
  /** Stays on screen until the next action. */
  actionError: string;
  busy: boolean;
  running: NetworkKind | null;
}

const EMPTY: PageSession = {
  state: null,
  stamp: 0,
  message: { summary: "", description: "" },
  offered: "",
  actionError: "",
  busy: false,
  running: null,
};
const sessions = new Map<string, PageSession>();
const requests = new Map<string, number>();
const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export const readSession = (workspaceId: string) =>
  sessions.get(workspaceId) ?? EMPTY;

/** Only the newest request may set the page, so a slow refresh that started
 * before an action can't put its old state back. */
export function nextRequest(workspaceId: string) {
  const request = (requests.get(workspaceId) ?? 0) + 1;
  requests.set(workspaceId, request);
  return request;
}
export const isNewestRequest = (workspaceId: string, request: number) =>
  requests.get(workspaceId) === request;

type Change =
  Partial<PageSession> | ((current: PageSession) => Partial<PageSession>);

export function usePageSession(workspaceId: string) {
  const session = useSyncExternalStore(subscribe, () =>
    readSession(workspaceId),
  );
  const update = useCallback(
    (change: Change) => {
      const current = readSession(workspaceId);
      const patch = typeof change === "function" ? change(current) : change;
      sessions.set(workspaceId, { ...current, ...patch });
      listeners.forEach((listener) => listener());
    },
    [workspaceId],
  );
  return [session, update] as const;
}
