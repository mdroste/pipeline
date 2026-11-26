/** Registers native events at the original lifecycle phase. Model transitions
 * own identity, terminal handling and streaming; this adapter owns disposal. */
import { useEffect } from "react";
import {
  decodeConversationEvent,
  listenConversationEvents,
} from "../../lib/workbench/events";
import type { DecodedConversationEvent } from "../../lib/workbench/events";
export interface ConversationEventIntents {
  finishSubmission(status: string, reason?: string | null): void;
  observeEvent(event: DecodedConversationEvent, disposed: () => boolean): void;
  onSubscribed(): unknown;
  disposeEvents(): void;
}
export function useWorkspaceConversationEvents(
  intents: ConversationEventIntents,
) {
  const { finishSubmission, observeEvent, onSubscribed, disposeEvents } =
    intents;
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenConversationEvents(({ payload }) => {
      if (!disposed)
        observeEvent(decodeConversationEvent(payload), () => disposed);
    }).then((stop) => {
      if (disposed) stop();
      else {
        unlisten = stop;
        void onSubscribed();
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
      disposeEvents();
    };
  }, [observeEvent, onSubscribed, disposeEvents]);
  return { finishSubmission };
}
