/** Conversation event transport. Raw extension payloads remain available; known
 * variants are projected only after their identity and required fields match.
 * Subscription disposal also covers a native registration that resolves late. */
import { listen, type EventCallback } from "@tauri-apps/api/event";
import type { WorkbenchEvent } from "./runtimeTypes";

interface TurnIdentity {
  epoch: number;
  threadId: string;
  turnId: string;
}
export interface KnownConversationEvents {
  agentMessageDelta: TurnIdentity & { itemId: string; delta: string };
  turnStarted: TurnIdentity;
  turnCompleted: TurnIdentity & { status: string };
  connectionClosed: { epoch: number; reason: string; retryable: boolean };
  protocolWarning: { epoch: number; message: string };
  sessionTitleUpdated: { sessionId: string };
}
export type KnownConversationEvent = {
  [K in keyof KnownConversationEvents]: WorkbenchEvent &
    KnownConversationEvents[K] & { kind: K };
}[keyof KnownConversationEvents];
export type DecodedConversationEvent =
  | { known: true; event: KnownConversationEvent }
  | { known: false; event: WorkbenchEvent };

export function decodeConversationEvent(
  event: WorkbenchEvent,
): DecodedConversationEvent {
  const turn =
    typeof event.epoch === "number" &&
    typeof event.threadId === "string" &&
    typeof event.turnId === "string";
  const valid = (() => {
    switch (event.kind) {
      case "agentMessageDelta":
        return (
          turn &&
          typeof event.itemId === "string" &&
          typeof event.delta === "string"
        );
      case "turnStarted":
        return turn;
      case "turnCompleted":
        return turn && typeof event.status === "string";
      case "connectionClosed":
        return (
          typeof event.epoch === "number" &&
          typeof event.reason === "string" &&
          typeof event.retryable === "boolean"
        );
      case "protocolWarning":
        return (
          typeof event.epoch === "number" && typeof event.message === "string"
        );
      case "sessionTitleUpdated":
        return typeof event.sessionId === "string";
      default:
        return false;
    }
  })();
  return valid
    ? { known: true, event: event as KnownConversationEvent }
    : { known: false, event };
}

export const listenConversationEvents = (
  handler: EventCallback<WorkbenchEvent>,
) => listen<WorkbenchEvent>("workbench:event", handler);

/** The caller owns setup-error presentation; disposal never delivers late events. */
export function subscribeConversationEvents(
  handler: (event: DecodedConversationEvent) => void,
  onError: (error: unknown) => void,
  register = listenConversationEvents,
) {
  let disposed = false;
  let release: (() => void) | undefined;
  void register(({ payload }) => {
    if (!disposed) handler(decodeConversationEvent(payload));
  }).then(
    (stop) => {
      if (disposed) stop();
      else release = stop;
    },
    (error: unknown) => {
      if (!disposed) onError(error);
    },
  );
  return () => {
    if (disposed) return;
    disposed = true;
    release?.();
    release = undefined;
  };
}
