import { useEffect, type Dispatch, type SetStateAction } from "react";
import { appEvents } from "../lib/appEvents";

/** External navigation requests reveal the requested pane without submitting work. */
export default function useWorkspaceEntry({
  workspaceId,
  entryRequest,
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
  entrySurface?: "chat" | "project";
  newProjectRequest: number;
  onNewProjectRequestHandled?: () => void;
  setSurface: Dispatch<SetStateAction<"chat" | "project">>;
  setProjectRequest: Dispatch<SetStateAction<number>>;
  setAssistantRequest: Dispatch<SetStateAction<number>>;
  setProjectDialog: Dispatch<SetStateAction<boolean>>;
}) {
  useEffect(() => {
    return appEvents.on("open-file", (detail) => {
      if (detail.workspaceId === workspaceId) setSurface("project");
    });
  }, [workspaceId, setSurface]);
  useEffect(() => {
    if (entryRequest <= 0 || !entrySurface) return;
    setSurface(entrySurface);
    if (entrySurface === "project") setProjectRequest((request) => request + 1);
    else setAssistantRequest((request) => request + 1);
  }, [
    entryRequest,
    entrySurface,
    setSurface,
    setProjectRequest,
    setAssistantRequest,
  ]);
  useEffect(() => {
    if (newProjectRequest <= 0) return;
    setProjectDialog(true);
    onNewProjectRequestHandled?.();
  }, [newProjectRequest, onNewProjectRequestHandled, setProjectDialog]);
}
