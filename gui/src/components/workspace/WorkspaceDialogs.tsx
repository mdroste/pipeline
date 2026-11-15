import { lazy, Suspense } from "react";
import type {
  Workspace,
  WorkbenchSession,
} from "../../lib/workbench/sessionTypes";
import type { WorkspaceDestination } from "../../lib/workspaceNavigation";
import WorkspaceToolPicker from "../WorkspaceToolPicker";
import { MoveConversationDialog } from "../WorkspaceConversationActions";

const WorkspaceProjectDialog = lazy(() => import("../WorkspaceProjectDialog"));

export default function WorkspaceDialogs({
  workspaceId,
  destination,
  toolPickerOpen,
  navigateProject,
  setToolPickerOpen,
  projectDialog,
  setProjectDialog,
  projectCreated,
  moveTarget,
  workspaces,
  moving,
  moveError,
  moveSession,
  setMoveTarget,
}: {
  workspaceId: string | null;
  destination: WorkspaceDestination;
  toolPickerOpen: boolean;
  navigateProject: (view: WorkspaceDestination) => void;
  setToolPickerOpen: (open: boolean) => void;
  projectDialog: boolean;
  setProjectDialog: (open: boolean) => void;
  projectCreated: (workspace: Workspace) => Promise<void>;
  moveTarget: WorkbenchSession | null;
  workspaces: Workspace[];
  moving: boolean;
  moveError: string | null;
  moveSession: (
    session: WorkbenchSession,
    target: string | null,
  ) => Promise<unknown>;
  setMoveTarget: (session: WorkbenchSession | null) => void;
}) {
  return (
    <>
      {toolPickerOpen && workspaceId && (
        <WorkspaceToolPicker
          workspaceId={workspaceId}
          current={destination}
          onChoose={navigateProject}
          onClose={() => setToolPickerOpen(false)}
        />
      )}

      {projectDialog && (
        <Suspense fallback={null}>
          <WorkspaceProjectDialog
            onClose={() => setProjectDialog(false)}
            onCreated={projectCreated}
          />
        </Suspense>
      )}
      {moveTarget && (
        <MoveConversationDialog
          key={moveTarget.id}
          session={moveTarget}
          workspaces={workspaces}
          busy={moving}
          error={moveError}
          onMove={(target) => void moveSession(moveTarget, target)}
          onClose={() => {
            if (!moving) setMoveTarget(null);
          }}
        />
      )}
    </>
  );
}
