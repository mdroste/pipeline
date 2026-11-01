import type { SetStateAction } from "react";

/**
 * The answer being streamed. It lives outside React state so a flush re-renders
 * only the message showing it, not the page that owns the conversation.
 */
export interface StreamStore {
  get: () => string;
  set: (next: SetStateAction<string>) => void;
  subscribe: (listener: () => void) => () => void;
}

export function createStreamStore(): StreamStore {
  let text = "";
  const listeners = new Set<() => void>();
  return {
    get: () => text,
    set: (next) => {
      const value = typeof next === "function" ? next(text) : next;
      if (value === text) return;
      text = value;
      for (const listener of listeners) listener();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
