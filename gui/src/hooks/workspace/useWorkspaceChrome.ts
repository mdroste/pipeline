import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
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
  };
  const closeInspector = (restoreFocus = true) => {
    if (contextBusyRef.current) return;
    setInspector(null);
    if (restoreFocus)
      requestAnimationFrame(() => inspectorTriggerRef.current?.focus());
  };
  return {
    inspector,
    setInspector,
    projectRequest,
    setProjectRequest,
    resetRequest,
    destination,
    setDestinationState,
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
) {
  const { setToolPickerOpen, surface } = chrome;
  useEffect(() => {
    setToolPickerOpen(false);
  }, [sessionId, workspaceId, surface]);
}
