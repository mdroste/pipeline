import { useCallback, useEffect, useState } from "react";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { WorkbenchSession } from "../lib/workbenchTypes";

export interface IndexedConversation {
  session: WorkbenchSession;
  projectName: string | null;
}

/**
 * Every conversation across projects, newest first. The sessions are read
 * project by project, so callers load this only while it is on screen.
 */
export default function useConversationIndex(
  enabled = true,
  includeArchived = false,
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
          const { sessions } = await conversationClient.listSessions(
            workspace?.id ?? null,
            includeArchived,
          );
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
  }, [enabled, includeArchived, request]);
  const refresh = useCallback(() => setRequest((value) => value + 1), []);
  return { conversations, loading, error, refresh };
}
