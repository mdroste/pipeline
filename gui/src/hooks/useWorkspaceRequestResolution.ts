import { useRef, type Dispatch, type SetStateAction } from "react";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { WorkbenchEvent } from "../lib/workbenchTypes";

export default function useWorkspaceRequestResolution(
  setRequests: Dispatch<SetStateAction<WorkbenchEvent[]>>,
  setError: Dispatch<SetStateAction<string | null>>,
) {
  const resolving = useRef(new Set<string>());
  const uncertain = useRef(new Set<string>());
  return async (event: WorkbenchEvent, result?: Record<string, unknown>) => {
    if (event.requestId === undefined || !event.method) return;
    const key = JSON.stringify([event.epoch, event.requestId]);
    if (resolving.current.has(key)) return;
    resolving.current.add(key);
    const matches = (item: WorkbenchEvent) =>
      item.requestId === event.requestId && item.epoch === event.epoch;
    try {
      // An earlier acknowledgement may have been lost. Confirm that the
      // original request is still pending before permitting another decision.
      if (uncertain.current.has(key)) {
        const pending = await workbenchClient.pendingRequests();
        setRequests(pending);
        uncertain.current.delete(key);
        if (!pending.some(matches)) return;
      }
      await workbenchClient.resolveServerRequest({
        epoch: event.epoch,
        requestId: event.requestId,
        method: event.method,
        result: result ?? null,
        declineMessage: result ? null : "The user declined this request",
      });
      setRequests((old) => old.filter((item) => !matches(item)));
    } catch (cause) {
      uncertain.current.add(key);
      setError(workbenchErrorMessage(cause));
      try {
        const pending = await workbenchClient.pendingRequests();
        setRequests(pending);
        uncertain.current.delete(key);
      } catch {
        setError(
          "Could not refresh pending approvals. Retry to check their current status before sending another decision.",
        );
      }
    } finally {
      resolving.current.delete(key);
    }
  };
}
