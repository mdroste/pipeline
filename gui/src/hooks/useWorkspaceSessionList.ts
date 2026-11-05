import {
  useCallback,
  useRef,
  useState,
  type RefObject,
  type Dispatch,
  type SetStateAction,
} from "react";
import { conversationClient } from "../lib/workbench/conversationClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  SessionCursor,
  SessionList,
  WorkbenchSession,
} from "../lib/workbenchTypes";

/** How far down a project's list the researcher has read. */
interface SessionPosition {
  workspace: string | null;
  pages: number;
  next: SessionCursor | null;
}

/** Reads older conversations onto the end of the list, one page at a time. */
export function useOlderSessions(
  workspace: RefObject<string | null>,
  sequence: RefObject<number>,
  setSessions: Dispatch<SetStateAction<WorkbenchSession[]>>,
  showArchived: boolean,
  onError: (message: string) => void,
) {
  const position = useRef<SessionPosition>({
    workspace: null,
    pages: 1,
    next: null,
  });
  const [hasOlder, setHasOlder] = useState(false);
  const reportError = useRef(onError);
  reportError.current = onError;
  const loadOlder = useCallback(async () => {
    const { workspace: selected, pages, next } = position.current;
    if (!next || selected !== workspace.current) return;
    const request = ++sequence.current;
    const stale = () =>
      request !== sequence.current || selected !== workspace.current;
    let response: SessionList;
    try {
      response = await conversationClient.listSessions(
        selected,
        showArchived,
        next,
      );
    } catch (cause) {
      if (!stale())
        reportError.current(
          `Could not load older conversations: ${workbenchErrorMessage(cause)}`,
        );
      return;
    }
    if (stale()) return;
    const after = response.nextCursor ?? null;
    position.current = { workspace: selected, pages: pages + 1, next: after };
    setHasOlder(after !== null);
    setSessions((old) => {
      const listed = new Set(old.map((session) => session.id));
      return [
        ...old,
        ...response.sessions.filter((session) => !listed.has(session.id)),
      ];
    });
  }, [workspace, sequence, setSessions, showArchived]);
  return { position, hasOlder, setHasOlder, loadOlder };
}

/**
 * Loads a project's conversation list. With `onError`, a failed load is
 * reported there and the current list stays in place; without it, the
 * failure propagates to the caller. `onError` may change between renders
 * without changing the returned loader. With `older`, a reload keeps the
 * older pages already on screen, so an open conversation stays listed. With
 * `keepSelection`, a load leaves the selected conversation alone even when the
 * list omits it.
 */
export default function useWorkspaceSessionList(
  workspace: RefObject<string | null>,
  sequence: RefObject<number>,
  setSessions: Dispatch<SetStateAction<WorkbenchSession[]>>,
  setSessionId: Dispatch<SetStateAction<string | null>>,
  showArchived: boolean,
  onError?: (message: string) => void,
  older?: Pick<ReturnType<typeof useOlderSessions>, "position" | "setHasOlder">,
) {
  const reportError = useRef(onError);
  reportError.current = onError;
  return useCallback(
    async (
      selectedWorkspace: string | null,
      archived = showArchived,
      keepSelection = false,
    ) => {
      const request = ++sequence.current;
      const stale = () =>
        request !== sequence.current || selectedWorkspace !== workspace.current;
      const saved = older?.position.current;
      const pages = saved?.workspace === selectedWorkspace ? saved.pages : 1;
      let sessions: WorkbenchSession[];
      let next: SessionCursor | null = null;
      let read = 0;
      try {
        sessions = [];
        do {
          const page: SessionList = await conversationClient.listSessions(
            selectedWorkspace,
            archived,
            next,
          );
          const listed = new Set(sessions.map((session) => session.id));
          sessions.push(
            ...page.sessions.filter((session) => !listed.has(session.id)),
          );
          next = page.nextCursor ?? null;
          read += 1;
        } while (next && read < pages);
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
      if (older) {
        older.position.current = {
          workspace: selectedWorkspace,
          pages: read,
          next,
        };
        older.setHasOlder(next !== null);
      }
      setSessions(sessions);
      if (keepSelection) return;
      setSessionId((current) =>
        sessions.some((session) => session.id === current)
          ? current
          : (sessions.find((session) => session.id === remembered)?.id ??
            sessions[0]?.id ??
            null),
      );
    },
    [workspace, sequence, setSessions, setSessionId, showArchived, older],
  );
}
