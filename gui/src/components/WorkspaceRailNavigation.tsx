import { useRef, useState } from "react";
import type { WorkspaceDestination } from "../lib/workspaceNavigation";
import type { WorkbenchSession, Workspace } from "../lib/workbenchTypes";
import Badge from "../ui/Badge";
import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";
import { Menu, MenuItem, MenuSeparator } from "../ui/Menu";
import { NavChild } from "./nav-rail/NavRows";
import { ConversationMenu } from "./WorkspaceConversationActions";
import WorkspaceProjectNavigation from "./WorkspaceProjectNavigation";

/** Conversations listed before "Show all"; the open one always stays listed. */
const SHOWN_SESSIONS = 3;

/**
 * The open project's place in the primary sidebar: its six sections, its
 * conversations, and the way to another project. The Workspace page portals
 * this into the sidebar, so every action here runs through the page's own
 * save and busy gates. With no project, it lists unfiled conversations.
 */
export default function WorkspaceRailNavigation({
  project,
  projects,
  destination,
  sessions,
  hasOlderSessions = false,
  onLoadOlderSessions,
  sessionId,
  chatOpen,
  background,
  disabled,
  onNavigate,
  onSelectSession,
  onCreateSession,
  onSelectProject,
  onNewProject,
  onAllProjects,
  conversationActions,
}: {
  project: Workspace | null;
  projects: Workspace[];
  destination: WorkspaceDestination;
  sessions: WorkbenchSession[];
  /** Older conversations remain past the list's last page. */
  hasOlderSessions?: boolean;
  onLoadOlderSessions?: () => void;
  sessionId: string | null;
  /** The conversation fills the page, so its row is the current location. */
  chatOpen: boolean;
  background: { sessionId: string; waiting: number } | null;
  disabled: boolean;
  onNavigate: (destination: WorkspaceDestination) => void;
  onSelectSession: (id: string) => void;
  onCreateSession: () => void;
  onSelectProject: (id: string) => void;
  onNewProject: () => void;
  onAllProjects?: () => void;
  conversationActions: (
    session: WorkbenchSession,
  ) => Pick<
    Parameters<typeof ConversationMenu>[0],
    | "onRename"
    | "onGenerateTitle"
    | "onMove"
    | "onRevealFiles"
    | "onArchive"
    | "onDelete"
  >;
}) {
  const [showAll, setShowAll] = useState(false);
  const [switching, setSwitching] = useState(false);
  const switcher = useRef<HTMLButtonElement>(null);
  const listed = showAll
    ? sessions
    : sessions.filter(
        (session, index) => index < SHOWN_SESSIONS || session.id === sessionId,
      );
  return (
    <>
      {project && (
        <>
          <NavChild
            ref={switcher}
            strong
            label={project.name}
            ariaLabel={`Switch project: ${project.name}`}
            title={
              project.missingRootAt
                ? `${project.name} · Folder not found`
                : (project.root ?? project.name)
            }
            disabled={disabled}
            aria-haspopup="menu"
            aria-expanded={switching}
            onClick={() => setSwitching((value) => !value)}
            suffix={
              project.missingRootAt ? (
                <Badge kind="attention" label="Folder not found" />
              ) : (
                <Icon name="chevron-down" className="nav-switch-mark" />
              )
            }
          />
          <Menu
            open={switching}
            onClose={() => setSwitching(false)}
            anchorRef={switcher}
            label="Switch project"
            side="right"
            align="start"
            width={256}
          >
            {projects
              .filter((item) => !item.archivedAt)
              .map((item) => (
                <MenuItem
                  key={item.id}
                  checked={item.id === project.id}
                  onSelect={() => {
                    if (item.id !== project.id) onSelectProject(item.id);
                  }}
                >
                  {item.name}
                </MenuItem>
              ))}
            <MenuSeparator />
            <MenuItem onSelect={() => onNavigate("project-settings")}>
              Project settings
            </MenuItem>
            <MenuItem onSelect={() => onAllProjects?.()}>All projects</MenuItem>
            <MenuItem onSelect={onNewProject}>New project</MenuItem>
          </Menu>
          <WorkspaceProjectNavigation
            key={project.id}
            destination={destination}
            disabled={disabled}
            onNavigate={onNavigate}
          />
        </>
      )}
      <div className="nav-label">
        <span>Conversations</span>
        <IconButton
          size="sm"
          className="!h-5 !w-5"
          label="New conversation"
          tooltipSide="right"
          disabled={disabled}
          onClick={onCreateSession}
        >
          <Icon name="plus" className="h-3.5 w-3.5" />
        </IconButton>
      </div>
      <nav aria-label="Conversations" className="nav-slot">
        {!sessions.length && <p className="nav-empty">No conversations yet.</p>}
        {listed.map((session) => {
          const running = background?.sessionId === session.id;
          return (
            <div key={session.id} className="nav-session">
              <NavChild
                label={session.title}
                current={session.id === sessionId && chatOpen}
                disabled={disabled}
                onClick={() => onSelectSession(session.id)}
                suffix={
                  running ? (
                    <Badge
                      kind={background.waiting ? "attention" : "running"}
                      label={
                        background.waiting
                          ? "Needs your approval"
                          : "Responding"
                      }
                    />
                  ) : session.archivedAt ? (
                    <span className="sr-only">Archived</span>
                  ) : undefined
                }
              />
              <span className="nav-session-menu">
                <ConversationMenu
                  session={session}
                  disabled={disabled}
                  {...conversationActions(session)}
                />
              </span>
            </div>
          );
        })}
        {sessions.length > SHOWN_SESSIONS && (
          <NavChild
            quiet
            label={
              showAll
                ? "Show fewer"
                : `Show all ${sessions.length}${hasOlderSessions ? "+" : ""}`
            }
            onClick={() => setShowAll((value) => !value)}
          />
        )}
        {showAll && hasOlderSessions && (
          <NavChild
            quiet
            label="Show older conversations"
            onClick={() => onLoadOlderSessions?.()}
          />
        )}
      </nav>
    </>
  );
}
