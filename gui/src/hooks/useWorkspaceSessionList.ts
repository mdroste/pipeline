import {
  useCallback,
  useRef,
  type RefObject,
  type Dispatch,
  type SetStateAction,
} from "react";
import { conversationClient } from "../lib/workbench/conversationClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { SessionList, WorkbenchSession } from "../lib/workbenchTypes";

/**
 * Loads a project's conversation list. With `onError`, a failed load is
 * reported there and the current list stays in place; without it, the
 * failure propagates to the caller. `onError` may change between renders
 * without changing the returned loader.
 */
export default function useWorkspaceSessionList(
  workspace: RefObject<string | null>,
  sequence: RefObject<number>,
  setSessions: Dispatch<SetStateAction<WorkbenchSession[]>>,
  setSessionId: Dispatch<SetStateAction<string | null>>,
  showArchived: boolean,
  onError?: (message: string) => void,
) {
  const reportError = useRef(onError);
  reportError.current = onError;
  return useCallback(
    async (selectedWorkspace: string | null, archived = showArchived) => {
      const request = ++sequence.current;
      const stale = () =>
        request !== sequence.current || selectedWorkspace !== workspace.current;
      let response: SessionList;
      try {
        response = await conversationClient.listSessions(
          selectedWorkspace,
          archived,
        );
      } catch (cause) {
        const report = reportError.current;
        if (!report) throw cause;
        if (!stale())
          report(
            `Could not load the conversation list: ${workbenchErrorMessage(cause)}`,
          );
        return;
      }
      if (stale()) return;
      let remembered: string | null = null;
      try {
        remembered = localStorage.getItem(
          `pipeline.workspace.lastSession.${selectedWorkspace ?? "unfiled"}`,
        );
      } catch {
        /* Optional navigation state. */
      }
      setSessions(response.sessions);
      setSessionId((current) =>
        response.sessions.some((session) => session.id === current)
          ? current
          : (response.sessions.find((session) => session.id === remembered)
              ?.id ??
            response.sessions[0]?.id ??
            null),
      );
    },
    [workspace, sequence, setSessions, setSessionId, showArchived],
  );
}
