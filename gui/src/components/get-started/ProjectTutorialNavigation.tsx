import WorkspaceRailNavigation from "../WorkspaceRailNavigation";
import type {
  Workspace,
  WorkbenchSession,
} from "../../lib/workbench/sessionTypes";
import type { WorkspaceDestination } from "../../lib/workspaceNavigation";

const noop = () => {};

/** The spotlight keeps this navigation inert; it replaces the live project's sidebar. */
export default function ProjectTutorialNavigation({
  workspace,
  session,
  destination,
}: {
  workspace: Workspace;
  session: WorkbenchSession;
  destination: WorkspaceDestination;
}) {
  return (
    <WorkspaceRailNavigation
      project={workspace}
      projects={[workspace]}
      sessions={[session]}
      destination={destination}
      sessionId={session.id}
      chatOpen={false}
      background={null}
      disabled={false}
      onNavigate={noop}
      onSelectSession={noop}
      onCreateSession={noop}
      onSelectProject={noop}
      onNewProject={noop}
      onAllProjects={noop}
      conversationActions={() => ({
        onRename: noop,
        onGenerateTitle: noop,
        onMove: noop,
        onRevealFiles: noop,
        onArchive: noop,
        onDelete: noop,
      })}
    />
  );
}
