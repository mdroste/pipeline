import { useLayoutEffect, useRef } from "react";
import type { WorkspaceDestination } from "../../lib/workspaceNavigation";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { useNavRailSlot } from "../nav-rail/navRailSlot";

export function useWorkspaceRail(
  workspaceId: string | null,
  projectName: string | null,
  createSession: () => Promise<unknown>,
  navigateProject: (destination: WorkspaceDestination) => void,
  setError: (message: string) => void,
) {
  // The sidebar's actions run the latest closures without re-reporting.
  const railActions = useRef({
    newConversation: () => undefined as void,
    openView: (_destination: WorkspaceDestination) => undefined as void,
  });
  railActions.current = {
    newConversation: () =>
      void createSession().catch((cause) =>
        setError(workbenchErrorMessage(cause)),
      ),
    openView: navigateProject,
  };
  const rail = useNavRailSlot();
  const setRailWorkspace = rail?.setWorkspace;
  useLayoutEffect(() => {
    setRailWorkspace?.({
      projectId: workspaceId,
      projectName,
      newConversation: () => railActions.current.newConversation(),
      openView: (view) => railActions.current.openView(view),
    });
    return () => setRailWorkspace?.(null);
  }, [setRailWorkspace, workspaceId, projectName]);
  const dismissRail = () => rail?.dismissRef.current?.();
  return { rail, dismissRail };
}
