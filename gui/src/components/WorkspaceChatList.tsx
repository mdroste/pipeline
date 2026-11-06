// Chat lists share the active Workspace's session actions and paged metadata.
// A separate outlet lets the project pane retain its editors while the list is open.
import {
  createContext,
  useContext,
  useState,
  type ComponentProps,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import useModalDialog from "../hooks/useModalDialog";
import type { WorkbenchSession } from "../lib/workbenchTypes";
import { ConversationMenu } from "./WorkspaceConversationActions";
import Badge from "../ui/Badge";

export const ProjectChatsContext = createContext<ReactNode>(null);
export function ProjectChatsOutlet({ fallback }: { fallback: ReactNode }) {
  return useContext(ProjectChatsContext) ?? fallback;
}
export function UnfiledChatsDialog({
  children,
  onClose,
}: {
  children: ReactNode;
  onClose: () => void;
}) {
  const ref = useModalDialog<HTMLDivElement>(onClose, true, true);
  return createPortal(
    <div
      className="workspace-picker-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label="Unfiled chats"
        tabIndex={-1}
        className="workspace-tool-picker"
      >
        <div className="workspace-picker-heading">
          <h2>No project</h2>
          <button type="button" aria-label="Close chats" onClick={onClose}>
            ×
          </button>
        </div>
        {children}
      </div>
    </div>,
    document.body,
  );
}
type Actions = Omit<
  ComponentProps<typeof ConversationMenu>,
  "session" | "disabled"
>;
export default function WorkspaceChatList({
  sessions,
  sessionId,
  hasOlderSessions,
  onLoadOlderSessions,
  disabled = false,
  showArchived = false,
  onArchived,
  onSelectSession,
  onCreateSession,
  conversationActions,
  background,
}: {
  sessions: WorkbenchSession[];
  sessionId?: string | null;
  hasOlderSessions?: boolean;
  onLoadOlderSessions?: () => void;
  disabled?: boolean;
  showArchived?: boolean;
  onArchived?: (value: boolean) => void;
  onSelectSession: (id: string) => void;
  onCreateSession?: () => void;
  conversationActions?: (session: WorkbenchSession) => Actions;
  background?: { sessionId: string; waiting: number } | null;
}) {
  const [query, setQuery] = useState("");
  const listed = sessions.filter(
    (s) =>
      Boolean(s.archivedAt) === showArchived &&
      s.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()),
  );
  return (
    <section aria-label="Chats" className="focused-page">
      <header>
        <div>
          <h1>Chats</h1>
          <p>Conversations and their working drafts.</p>
        </div>
        {onCreateSession && (
          <button
            type="button"
            className="ui-page-button primary"
            disabled={disabled}
            onClick={onCreateSession}
          >
            New chat
          </button>
        )}
      </header>
      <input
        className="focused-search"
        type="search"
        aria-label="Find a chat"
        placeholder="Find a chat…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      {onArchived && (
        <div className="focused-tabs" role="group" aria-label="Show chats">
          {["Current", "Archived"].map((label, i) => (
            <button
              type="button"
              key={label}
              disabled={disabled}
              aria-pressed={Boolean(i) === showArchived}
              onClick={() => onArchived(Boolean(i))}
            >
              {label}
            </button>
          ))}
        </div>
      )}
      <ul className="focused-chat-list">
        {listed.map((session) => (
          <li key={session.id}>
            <button
              type="button"
              className="focused-chat-open"
              disabled={disabled}
              aria-current={session.id === sessionId ? "true" : undefined}
              onClick={() => onSelectSession(session.id)}
            >
              <strong>{session.title}</strong>
              <small>
                {[
                  session.id === sessionId ? "Current chat" : null,
                  session.draft ? "Unsent draft" : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </small>
            </button>
            {background?.sessionId === session.id && (
              <Badge
                kind={background.waiting ? "attention" : "running"}
                label={
                  background.waiting ? "Needs your approval" : "Responding"
                }
              />
            )}
            {conversationActions && (
              <ConversationMenu
                session={session}
                disabled={disabled}
                {...conversationActions(session)}
              />
            )}
          </li>
        ))}
      </ul>
      {!listed.length && (
        <p role="status">
          {query
            ? "No matching chats on these pages."
            : showArchived
              ? "No archived chats on these pages."
              : "No conversations yet."}
        </p>
      )}
      {hasOlderSessions && (
        <button
          type="button"
          className="ui-page-button"
          disabled={disabled}
          onClick={onLoadOlderSessions}
        >
          Show older conversations
        </button>
      )}
    </section>
  );
}
