import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { appEvents } from "../lib/appEvents";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  SessionCursor,
  SessionList,
  WorkbenchEvent,
  WorkbenchSession,
} from "../lib/workbenchTypes";

/** Host events after which a title or the newest-first order may have changed. */
const CHANGES_THE_LIST = new Set(["sessionTitleUpdated", "turnCompleted"]);

export interface IndexedConversation {
  session: WorkbenchSession;
  projectName: string | null;
}

/**
 * Every conversation across projects, newest first. The sessions are read
 * project by project, so callers load this only while it is on screen, and it
 * stays current for as long as they do. A caller that shows only the newest can
 * leave `everyPage` off.
 */
export default function useConversationIndex(
  enabled = true,
  includeArchived = false,
  everyPage = false,
) {
  const [conversations, setConversations] = useState<IndexedConversation[]>([]);
  const [loading, setLoading] = useState(enabled);
  const [error, setError] = useState<string | null>(null);
  const [request, setRequest] = useState(0);
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    setLoading(true);
    void (async () => {
      const [{ workspaceClient }, { conversationClient }] = await Promise.all([
        import("../lib/workbench/workspaceClient"),
        import("../lib/workbench/conversationClient"),
      ]);
      const { workspaces } = await workspaceClient.listWorkspaces(false);
      const lists = await Promise.all(
        [null, ...workspaces].map(async (workspace) => {
          const sessions: WorkbenchSession[] = [];
          let cursor: SessionCursor | null = null;
          do {
            const page: SessionList = await conversationClient.listSessions(
              workspace?.id ?? null,
              includeArchived,
              cursor,
            );
            sessions.push(...page.sessions);
            cursor = (everyPage && page.nextCursor) || null;
          } while (cursor);
          return sessions.map((session) => ({
            session,
            projectName: workspace?.name ?? null,
          }));
        }),
      );
      return lists
        .flat()
        .sort((a, b) => b.session.updatedAt.localeCompare(a.session.updatedAt));
    })()
      .then((listed) => {
        if (!live) return;
        setConversations(listed);
        setError(null);
      })
      .catch((cause) => {
        if (live) setError(workbenchErrorMessage(cause));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [enabled, includeArchived, everyPage, request]);
  const refresh = useCallback(() => setRequest((value) => value + 1), []);
  useEffect(() => {
    if (!enabled) return;
    const offChanged = appEvents.on("conversations-changed", refresh);
    const offHost = listen<WorkbenchEvent>("workbench:event", ({ payload }) => {
      if (CHANGES_THE_LIST.has(payload.kind)) refresh();
    }).catch(() => null);
    return () => {
      offChanged();
      void offHost.then((stop) => stop?.());
    };
  }, [enabled, refresh]);
  return { conversations, loading, error, refresh };
}
