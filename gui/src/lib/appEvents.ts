// Typed in-app event bus for cross-component commands that are not
// navigation (navigation goes through lib/router). Replaces the untyped
// window CustomEvent channels.

import type { FileLocation } from "./fileLinks";
import type { OpenResearchObject } from "./deskClient";

export interface AppEventMap {
  /** Reveal a project destination tab inside an open workspace. */
  "workspace-destination": { workspaceId: string; tab: string };
  /** Open a project file at a location (from conversation file links). */
  "open-file": { workspaceId: string; location: FileLocation };
  /**
   * Add research objects to the conversation's sources. `whenReady` holds
   * them until the conversation can accept changes, for adds that follow an
   * import; without it an add made at the wrong moment is refused.
   */
  "context-add": {
    workspaceId: string;
    objects: OpenResearchObject[];
    whenReady?: boolean;
  };
}

type Listener<K extends keyof AppEventMap> = (payload: AppEventMap[K]) => void;

const listeners = new Map<keyof AppEventMap, Set<Listener<never>>>();

export const appEvents = {
  emit<K extends keyof AppEventMap>(event: K, payload: AppEventMap[K]) {
    const set = listeners.get(event) as Set<Listener<K>> | undefined;
    if (!set) return;
    for (const listener of [...set]) listener(payload);
  },
  on<K extends keyof AppEventMap>(event: K, listener: Listener<K>): () => void {
    let set = listeners.get(event) as Set<Listener<K>> | undefined;
    if (!set) {
      set = new Set();
      listeners.set(event, set as Set<Listener<never>>);
    }
    set.add(listener);
    return () => {
      set.delete(listener);
    };
  },
};
