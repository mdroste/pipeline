import { useState, type RefObject } from "react";
import { confirmDialog, promptDialog } from "../../components/DialogService";
import { conversationClient } from "../../lib/workbench/conversationClient";
import { newOperation } from "../../lib/workspaceSessionState";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type {
  WorkbenchSession,
  ConversationSnapshot,
} from "../../lib/workbenchTypes";

interface Props {
  workspaceId: string | null;
  showArchived: boolean;
  sessionRef: RefObject<string | null>;
  saveCurrentDraft: () => Promise<void>;
  loadSessions: (
    workspaceId: string | null,
    archived?: boolean,
  ) => Promise<void>;
  refreshSessionRecord: (id: string) => Promise<void>;
  hydrate: (id?: string | null) => Promise<void>;
  setSnapshot: (snapshot: ConversationSnapshot | null) => void;
  setSessionId: (id: string | null) => void;
  setDraft: (draft: string) => void;
  setSessions: (sessions: WorkbenchSession[]) => void;
  setWorkspaceId: (id: string | null) => void;
  setError: (error: string | null) => void;
}
export function useWorkspaceSessionMutations({
  workspaceId,
  showArchived,
  sessionRef,
  saveCurrentDraft,
  loadSessions,
  refreshSessionRecord,
  hydrate,
  setSnapshot,
  setSessionId,
  setDraft,
  setSessions,
  setWorkspaceId,
  setError,
}: Props) {
  const [moveTarget, setMoveTarget] = useState<WorkbenchSession | null>(null);
  const [moving, setMoving] = useState(false);
  const [moveError, setMoveError] = useState<string | null>(null);
  const archiveSession = async (session: WorkbenchSession) => {
    const latest = await conversationClient.sessionSnapshot(session.id);
    await conversationClient.updateSession({
      sessionId: session.id,
      expectedRevision: latest.session.revision,
      operationId: newOperation("archive-session"),
      archived: !latest.session.archivedAt,
    });
    await loadSessions(workspaceId, showArchived);
    if (sessionRef.current === session.id)
      await refreshSessionRecord(session.id);
  };

  const renameSession = async (session: WorkbenchSession) => {
    const answer = await promptDialog(
      "Choose a new title for this conversation.",
      {
        title: "Rename conversation",
        label: "Conversation title",
        defaultValue: session.title,
        confirmLabel: "Rename",
      },
    );
    if (answer === null) return;
    const title = answer.trim();
    if (!title || title === session.title) return;
    const latest = await conversationClient.sessionSnapshot(session.id);
    await conversationClient.updateSession({
      sessionId: session.id,
      expectedRevision: latest.session.revision,
      operationId: newOperation("rename-session"),
      title,
    });
    await refreshSessionRecord(session.id);
  };

  const generateTitle = async (session: WorkbenchSession) => {
    setError(null);
    await conversationClient.generateSessionTitle(session.id);
    await refreshSessionRecord(session.id);
  };

  const deleteSession = async (session: WorkbenchSession) => {
    if (
      !(await confirmDialog(
        `Delete “${session.title}”? Its transcript and ChatGPT history are removed permanently. Research records it produced stay with their project.`,
        {
          title: "Delete conversation",
          confirmLabel: "Delete",
          destructive: true,
        },
      ))
    )
      return;
    await conversationClient.deleteSession({
      sessionId: session.id,
      operationId: newOperation("delete-session"),
    });
    if (sessionRef.current === session.id) {
      setSnapshot(null);
      setSessionId(null);
      setDraft("");
    }
    await loadSessions(workspaceId, showArchived);
  };

  const moveSession = async (
    session: WorkbenchSession,
    target: string | null,
  ) => {
    setMoving(true);
    setMoveError(null);
    try {
      await saveCurrentDraft();
      const latest = await conversationClient.sessionSnapshot(session.id);
      await conversationClient.moveSession({
        sessionId: session.id,
        expectedRevision: latest.session.revision,
        operationId: newOperation("move-session"),
        workspaceId: target,
      });
      setMoveTarget(null);
      if (sessionRef.current === session.id) {
        // Follow the conversation into its new project instead of losing it
        // behind the sidebar filter.
        if (target !== workspaceId) {
          setSessions([]);
          setWorkspaceId(target);
        }
        await hydrate(session.id);
      } else {
        await loadSessions(workspaceId, showArchived);
      }
    } catch (cause) {
      setMoveError(workbenchErrorMessage(cause));
    } finally {
      setMoving(false);
    }
  };

  return {
    moveTarget,
    setMoveTarget,
    moving,
    moveError,
    setMoveError,
    archiveSession,
    renameSession,
    generateTitle,
    deleteSession,
    moveSession,
  };
}
