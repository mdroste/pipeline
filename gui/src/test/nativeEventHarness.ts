import type { EventCallback } from "@tauri-apps/api/event";

/** A disposable subscription and an explicit registration barrier for races. */
export function nativeEventHarness<T>() {
  const listeners = new Set<EventCallback<T>>();
  const registrations: Array<() => void> = [];
  return {
    register(handler: EventCallback<T>) {
      listeners.add(handler);
      return new Promise<() => void>((resolve) => {
        registrations.push(() =>
          resolve(() => {
            listeners.delete(handler);
          }),
        );
      });
    },
    ready() {
      for (const resolve of registrations.splice(0)) resolve();
    },
    reset() {
      listeners.clear();
      for (const resolve of registrations.splice(0)) resolve();
    },
    emit(payload: T) {
      for (const handler of [...listeners])
        handler({ event: "workbench:event", id: 1, payload });
    },
    get subscribers() {
      return listeners.size;
    },
  };
}
