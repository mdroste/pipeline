import {
  useCallback,
  type RefObject,
  type Dispatch,
  type SetStateAction,
} from "react";
import { conversationClient } from "../lib/workbench/conversationClient";
import type { WorkbenchSession } from "../lib/workbenchTypes";

export default function useWorkspaceSessionList(
  workspace: RefObject<string | null>,
  sequence: RefObject<number>,
  setSessions: Dispatch<SetStateAction<WorkbenchSession[]>>,
  setSessionId: Dispatch<SetStateAction<string | null>>,
  showArchived: boolean,
) {
  return useCallback(
    async (selectedWorkspace: string | null, archived = showArchived) => {
      const request = ++sequence.current;
      const response = await conversationClient.listSessions(
        selectedWorkspace,
        archived,
      );
      if (
        request !== sequence.current ||
        selectedWorkspace !== workspace.current
      )
        return;
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
