import { useCallback, useRef, useState } from "react";
import type { ActiveTurn, WorkbenchEvent } from "../../lib/workbenchTypes";

/** Events after which the host's running turn may have changed. */
const TURN_EVENTS = new Set([
  "activeTurnChanged",
  "turnStarted",
  "turnCompleted",
  "connectionClosed",
]);

/**
 * The app's one running turn. The host owns it, so it survives every page
 * change: a consumer reads it after subscribing to `workbench:event`, then
 * passes each event to `observeActiveTurn`. Undefined means not read yet.
 */
export function useActiveTurn(read: () => Promise<ActiveTurn | null>) {
  const [activeTurn, setActiveTurn] = useState<ActiveTurn | null | undefined>(
    undefined,
  );
  const lastRequest = useRef(0);
  const lastApplied = useRef(0);
  /** Resolves to undefined when the host could not be asked. */
  const refreshActiveTurn = useCallback(async () => {
    const request = ++lastRequest.current;
    let turn: ActiveTurn | null;
    try {
      turn = (await read()) ?? null;
    } catch {
      return undefined;
    }
    // Answers may overtake each other. Every one that is newer than the last
    // applied still lands, so a caller's answer is on screen when it returns.
    if (request > lastApplied.current) {
      lastApplied.current = request;
      setActiveTurn(turn);
    }
    return turn;
  }, [read]);
  const observeActiveTurn = useCallback(
    (event: WorkbenchEvent) => {
      if (TURN_EVENTS.has(event.kind)) void refreshActiveTurn();
    },
    [refreshActiveTurn],
  );
  return { activeTurn, refreshActiveTurn, observeActiveTurn };
}
