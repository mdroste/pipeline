import {
  createContext,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import type { WorkspaceDestination } from "../../lib/workspaceNavigation";

/** What the open Workspace page tells the sidebar. */
export interface WorkspaceRailReport {
  /** null while an unfiled conversation is open. */
  projectId: string | null;
  projectName: string | null;
  newConversation: () => void;
  openView: (destination: WorkspaceDestination) => void;
}

interface NavRailSlotValue {
  /** The sidebar element the open Workspace renders its navigation into. */
  slot: HTMLElement | null;
  setSlot: (element: HTMLElement | null) => void;
  workspace: WorkspaceRailReport | null;
  setWorkspace: (report: WorkspaceRailReport | null) => void;
  /** Closes the icon rail's flyout after a choice made inside the slot. */
  dismissRef: RefObject<(() => void) | null>;
}

const NavRailSlotContext = createContext<NavRailSlotValue | null>(null);

/**
 * Joins two siblings: the sidebar in the app shell and the lazily loaded
 * Workspace page. The page keeps owning its sessions, save gates, and busy
 * checks, and only portals its navigation into a place the sidebar provides.
 */
export function NavRailSlotProvider({ children }: { children: ReactNode }) {
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const [workspace, setWorkspace] = useState<WorkspaceRailReport | null>(null);
  const dismissRef = useRef<(() => void) | null>(null);
  const value = useMemo(
    () => ({ slot, setSlot, workspace, setWorkspace, dismissRef }),
    [slot, workspace],
  );
  return (
    <NavRailSlotContext.Provider value={value}>
      {children}
    </NavRailSlotContext.Provider>
  );
}

export const useNavRailSlot = () => useContext(NavRailSlotContext);
