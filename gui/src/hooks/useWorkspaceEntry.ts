import {
  useCallback,
  useEffect,
  type Dispatch,
  type SetStateAction,
} from "react";
import { appEvents } from "../lib/appEvents";

/** External navigation requests reveal the requested pane without submitting work. */
export default function useWorkspaceEntry({
  workspaceId,
  entryRequest,
  entryApplied,
  entrySurface,
  newProjectRequest,
  onNewProjectRequestHandled,
  setSurface,
  setProjectRequest,
  setAssistantRequest,
  setProjectDialog,
}: {
  workspaceId: string | null;
  entryRequest: number;
  /** The entry the page has finished applying, so its project is on screen. */
  entryApplied: number;
  entrySurface?: "chat" | "project";
  newProjectRequest: number;
  onNewProjectRequestHandled?: () => void;
  setSurface: Dispatch<SetStateAction<"chat" | "project">>;
  setProjectRequest: Dispatch<SetStateAction<number>>;
  setAssistantRequest: Dispatch<SetStateAction<number>>;
  setProjectDialog: Dispatch<SetStateAction<boolean>>;
}) {
  // The desk shows the project pane only on a request; the surface alone leaves
  // it hidden behind a chat that fills the page.
  const revealProject = useCallback(
    (detail: { workspaceId: string }) => {
      if (detail.workspaceId !== workspaceId) return;
      setSurface("project");
      setProjectRequest((request) => request + 1);
    },
    [workspaceId, setSurface, setProjectRequest],
  );
  useEffect(() => {
    const offDestination = appEvents.on("workspace-destination", revealProject);
    const offFile = appEvents.on("open-file", revealProject);
    const offObject = appEvents.on("open-object", revealProject);
    return () => {
      offDestination();
      offFile();
      offObject();
    };
  }, [revealProject]);
  useEffect(() => {
    if (entryRequest <= 0 || !entrySurface) return;
    setSurface(entrySurface);
  }, [entryRequest, entrySurface, setSurface]);
  useEffect(() => {
    if (entryApplied <= 0 || !entrySurface) return;
    if (entrySurface === "project") setProjectRequest((request) => request + 1);
    else setAssistantRequest((request) => request + 1);
  }, [entryApplied, setProjectRequest, setAssistantRequest]);
  useEffect(() => {
    if (newProjectRequest <= 0) return;
    setProjectDialog(true);
    onNewProjectRequestHandled?.();
  }, [newProjectRequest, onNewProjectRequestHandled, setProjectDialog]);
}
