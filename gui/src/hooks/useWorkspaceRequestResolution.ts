import { useRef, type Dispatch, type SetStateAction } from "react";
import { runtimeClient } from "../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { WorkbenchEvent } from "../lib/workbenchTypes";
import { reportProblem } from "./workspace/reportProblem";

export default function useWorkspaceRequestResolution(
  setRequests: Dispatch<SetStateAction<WorkbenchEvent[]>>,
  problems: Parameters<typeof reportProblem>[0],
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
    // The researcher may have left this conversation by the time it fails.
    const owner = event.sessionId ?? problems.sessionRef.current;
    const report = (message: string) =>
      owner
        ? reportProblem(problems, owner, message)
        : problems.setError(message);
    try {
      // An earlier acknowledgement may have been lost. Confirm that the
      // original request is still pending before permitting another decision.
      if (uncertain.current.has(key)) {
        const pending = await runtimeClient.pendingRequests();
        setRequests(pending);
        uncertain.current.delete(key);
        if (!pending.some(matches)) return;
      }
      await runtimeClient.resolveServerRequest({
        epoch: event.epoch,
        requestId: event.requestId,
        method: event.method,
        result: result ?? null,
        declineMessage: result ? null : "The user declined this request",
      });
      setRequests((old) => old.filter((item) => !matches(item)));
    } catch (cause) {
      uncertain.current.add(key);
      report(workbenchErrorMessage(cause));
      try {
        const pending = await runtimeClient.pendingRequests();
        setRequests(pending);
        uncertain.current.delete(key);
      } catch {
        report(
          "Could not refresh pending approvals. Retry to check their current status before sending another decision.",
        );
      }
    } finally {
      resolving.current.delete(key);
    }
  };
}
