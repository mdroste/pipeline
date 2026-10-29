import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import useContainerWidth from "../useContainerWidth";
import usePersistentPanelWidth from "../usePersistentPanelWidth";
import useModalDialog from "../useModalDialog";
import { loadDeskLayout } from "../../lib/deskLayout";
import { appEvents } from "../../lib/appEvents";
import {
  isWorkspaceDestination,
  type WorkspaceDestination,
} from "../../lib/workspaceNavigation";

export function useWorkspaceChrome(
  workspaceId: string | null,
  entrySurface: "chat" | "project" | undefined,
  contextBusyRef: RefObject<boolean>,
) {
  const [sidebarWidth, setSidebarWidth] = usePersistentPanelWidth(
    "pipeline.workspace.sidebarWidth",
    224,
    200,
    440,
  );
  const [inspector, setInspector] = useState<
    "settings" | "outline" | "context" | "activity" | null
  >(null);
  const [projectRequest, setProjectRequest] = useState(0);
  const resetRequest = 0;
  const [destinationState, setDestinationState] = useState<{
    workspaceId: string | null;
    tab: WorkspaceDestination;
  } | null>(null);
  const savedDestination = useMemo(
    () => (workspaceId ? loadDeskLayout(workspaceId).tab : "overview"),
    [workspaceId],
  );
  const destination =
    destinationState?.workspaceId === workspaceId
      ? destinationState.tab
      : isWorkspaceDestination(savedDestination)
        ? savedDestination
        : "overview";
  const [shellRef, shellWidth] = useContainerWidth<HTMLDivElement>();
  const [navigationOpen, setNavigationOpen] = useState(false);
  const navigationButtonRef = useRef<HTMLButtonElement>(null);
  const [navigationPinned, setNavigationPinned] = useState(() => {
    try {
      return (
        localStorage.getItem("pipeline.workspace.navigationPinned") === "true"
      );
    } catch {
      return false;
    }
  });
  const [toolPickerOpen, setToolPickerOpen] = useState(false);
  const conversationMenuRef = useRef<HTMLButtonElement>(null);
  const [assistantRequest, setAssistantRequest] = useState(0);

  const [harnessEditor, setHarnessEditor] = useState(false);
  const inspectorTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [surface, setSurface] = useState<"chat" | "project">(
    () =>
      entrySurface ??
      (Boolean(localStorage.getItem("pipeline.workspace.workspaceId"))
        ? "project"
        : "chat"),
  );
  const [projectDialog, setProjectDialog] = useState(false);
  const recordDestination = useCallback(
    (tab: WorkspaceDestination) => {
      setDestinationState({ workspaceId, tab });
    },
    [workspaceId],
  );
  const navigateProject = (tab: WorkspaceDestination) => {
    recordDestination(tab);
    setSurface("project");
    setNavigationOpen(false);
    // Re-selecting the current destination also leaves an opened object view.
    if (workspaceId)
      appEvents.emit("workspace-destination", { workspaceId, tab });
    setProjectRequest((value) => value + 1);
  };
  const openInspector = (
    next: "settings" | "outline" | "context" | "activity",
    trigger?: HTMLButtonElement,
  ) => {
    if (contextBusyRef.current || harnessEditor) return;
    if (trigger) inspectorTriggerRef.current = trigger;
    setInspector((value) => (value === next ? null : next));
    setAssistantRequest((value) => value + 1);
    setNavigationOpen(false);
  };
  const closeInspector = (restoreFocus = true) => {
    if (contextBusyRef.current) return;
    setInspector(null);
    if (restoreFocus)
      requestAnimationFrame(() => inspectorTriggerRef.current?.focus());
  };
  const navigationFits =
    shellWidth === null ||
    shellWidth - sidebarWidth >= (workspaceId ? 848 : 480);
  const navigationCollapsed = !navigationPinned || !navigationFits;
  const pinNavigation = (value: boolean) => {
    setNavigationPinned(value);
    setNavigationOpen(false);
    try {
      localStorage.setItem(
        "pipeline.workspace.navigationPinned",
        String(value),
      );
    } catch {
      /* Optional view preference. */
    }
    navigationButtonRef.current?.focus();
  };
  return {
    sidebarWidth,
    setSidebarWidth,
    inspector,
    setInspector,
    projectRequest,
    setProjectRequest,
    resetRequest,
    destination,
    setDestinationState,
    shellRef,
    shellWidth,
    navigationOpen,
    setNavigationOpen,
    navigationButtonRef,
    navigationPinned,
    toolPickerOpen,
    setToolPickerOpen,
    conversationMenuRef,
    assistantRequest,
    setAssistantRequest,
    harnessEditor,
    setHarnessEditor,
    inspectorTriggerRef,
    surface,
    setSurface,
    projectDialog,
    setProjectDialog,
    recordDestination,
    navigateProject,
    openInspector,
    closeInspector,
    navigationFits,
    navigationCollapsed,
    pinNavigation,
  };
}
export type WorkspaceChrome = ReturnType<typeof useWorkspaceChrome>;

export function useWorkspaceChromeSession(
  chrome: WorkspaceChrome,
  sessionId: string | null,
) {
  const { setHarnessEditor, setInspector } = chrome;
  // The harness editor is conversation-scoped; leaving the conversation closes it.
  useEffect(() => {
    setHarnessEditor(false);
    setInspector(null);
  }, [sessionId]);
}

export function useWorkspaceSurfacePreference(surface: "chat" | "project") {
  useEffect(() => {
    localStorage.setItem("pipeline.workspace.surface", surface);
  }, [surface]);
}

export function useWorkspaceChromeEffects(
  chrome: WorkspaceChrome,
  workspaceId: string | null,
  sessionId: string | null,
  contextBusy: boolean,
) {
  const {
    navigationCollapsed,
    navigationOpen,
    setNavigationOpen,
    setToolPickerOpen,
    surface,
    harnessEditor,
    toolPickerOpen,
  } = chrome;
  const navigationDialog = useModalDialog<HTMLDivElement>(
    () => setNavigationOpen(false),
    navigationCollapsed && navigationOpen,
    true,
  );
  useEffect(() => {
    setNavigationOpen(false);
    setToolPickerOpen(false);
  }, [sessionId, workspaceId, surface]);
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if (
        (event.metaKey || event.ctrlKey) &&
        event.key.toLowerCase() === "k" &&
        !event.altKey &&
        workspaceId &&
        !harnessEditor &&
        !contextBusy
      ) {
        // Keep shortcuts local to the project; another modal owns its keyboard.
        if (document.querySelector('[aria-modal="true"]') && !toolPickerOpen)
          return;
        event.preventDefault();
        setToolPickerOpen((value) => !value);
        setNavigationOpen(false);
      }
    };
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  }, [workspaceId, harnessEditor, contextBusy, toolPickerOpen]);

  return navigationDialog;
}
