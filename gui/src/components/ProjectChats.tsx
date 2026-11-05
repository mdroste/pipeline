import { useState } from "react";
import type { IndexedConversation } from "../hooks/useConversationIndex";
import { appEvents } from "../lib/appEvents";
import { projectDate, type ProjectIndexItem } from "../lib/projectIndex";
import { conversationClient } from "../lib/workbench/conversationClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { WorkbenchSession } from "../lib/workbenchTypes";
import { newOperation } from "../lib/workspaceSessionState";
import ReportIssueLink from "./ReportIssueLink";
import { MoveConversationDialog } from "./WorkspaceConversationActions";

export type ChatFilter = "all" | "unfiled" | "archived";
const FILTERS: Array<[ChatFilter, string]> = [
  ["all", "All chats"],
  ["unfiled", "No project"],
  ["archived", "Archived"],
];

/** Chats share the Projects home, but project membership is always optional. */
export default function ProjectChats({
  conversations,
  projects,
  loading,
  error,
  search,
  filter,
  onFilter,
  onOpen,
  onOpenProject,
}: {
  conversations: IndexedConversation[];
  projects: ProjectIndexItem[];
  loading: boolean;
  error: string | null;
  search: string;
  filter: ChatFilter;
  onFilter: (filter: ChatFilter) => void;
  onOpen: (id: string) => Promise<unknown>;
  onOpenProject: (id: string) => Promise<void>;
}) {
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [moveTarget, setMoveTarget] = useState<WorkbenchSession | null>(null);
  const [moveError, setMoveError] = useState<string | null>(null);
  const [moving, setMoving] = useState(false);
  const term = search.trim().toLocaleLowerCase();
  const visible = conversations.filter(({ session, projectName }) => {
    if (Boolean(session.archivedAt) !== (filter === "archived")) return false;
    if (filter === "unfiled" && session.workspaceId) return false;
    return `${session.title} ${projectName ?? ""}`
      .toLocaleLowerCase()
      .includes(term);
  });
  const open = async (action: () => Promise<unknown>) => {
    if (pending) return;
    setPending(true);
    setActionError(null);
    try {
      await action();
    } catch (cause) {
      setActionError(workbenchErrorMessage(cause));
    } finally {
      setPending(false);
    }
  };
  const move = async (workspaceId: string | null) => {
    if (!moveTarget || moving) return;
    setMoving(true);
    setMoveError(null);
    try {
      const latest = await conversationClient.sessionSnapshot(moveTarget.id);
      await conversationClient.moveSession({
        sessionId: moveTarget.id,
        workspaceId,
        expectedRevision: latest.session.revision,
        operationId: newOperation("move-session"),
      });
      appEvents.emit("conversations-changed", {});
      setMoveTarget(null);
    } catch (cause) {
      setMoveError(workbenchErrorMessage(cause));
    } finally {
      setMoving(false);
    }
  };
  return (
    <section className="project-index-chats" aria-label="Chats">
      <div className="project-index-section-heading">
        <h2>Chats</h2>
        <p className="project-home-meta">
          Pick up any conversation, with or without a project.
        </p>
      </div>
      <div
        role="group"
        aria-label="Show chats"
        className="project-chat-filters"
      >
        {FILTERS.map(([value, label]) => (
          <button
            key={value}
            type="button"
            className="ui-page-button"
            aria-pressed={filter === value}
            onClick={() => onFilter(value)}
          >
            {label}
          </button>
        ))}
      </div>
      {(error || actionError) && (
        <p role="alert" className="project-home-error">
          {actionError ?? error}
          <ReportIssueLink />
        </p>
      )}
      {loading && !visible.length && (
        <p role="status" className="project-home-empty">
          Loading chats…
        </p>
      )}
      {!loading && !error && !visible.length && (
        <p role="status" className="project-home-empty">
          {term
            ? `No chats match “${search}”.`
            : filter === "archived"
              ? "No archived chats."
              : filter === "unfiled"
                ? "No chats without a project. Start a new chat here anytime."
                : "No chats yet. Start a new chat, or open a project to chat there."}
        </p>
      )}
      {visible.map(({ session, projectName }) => (
        <article className="ui-page-row project-chat-row" key={session.id}>
          <button
            type="button"
            className="project-index-name"
            disabled={pending}
            onClick={() => void open(() => onOpen(session.id))}
          >
            {session.title}
          </button>
          <p className="project-home-meta">
            {session.workspaceId ? (
              <button
                type="button"
                className="project-home-link"
                aria-label={`Open project ${projectName}`}
                disabled={pending}
                onClick={() =>
                  void open(() => onOpenProject(session.workspaceId!))
                }
              >
                {projectName}
              </button>
            ) : (
              "No project"
            )}
            {" · "}
            {projectDate(session.updatedAt)}
            {session.archivedAt ? " · Archived" : ""}
          </p>
          <button
            type="button"
            className="project-home-link muted project-chat-move"
            aria-label={`Move ${session.title} to project`}
            disabled={pending}
            onClick={() => {
              setMoveError(null);
              setMoveTarget(session);
            }}
          >
            {session.workspaceId ? "Change project…" : "Move to project…"}
          </button>
        </article>
      ))}
      {moveTarget && (
        <MoveConversationDialog
          session={moveTarget}
          workspaces={projects.map((project) => ({
            ...project,
            archivedAt: null,
          }))}
          busy={moving}
          error={moveError}
          onMove={(id) => void move(id)}
          onClose={() => setMoveTarget(null)}
        />
      )}
    </section>
  );
}
