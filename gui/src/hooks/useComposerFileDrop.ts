import { useEffect, useRef, useState, type RefObject } from "react";
import { onFileDrop } from "../lib/fileDrop";
import { importResearchFiles, paperSource } from "../lib/workspaceAttachments";
import { addContextObjectsWhenReady } from "../components/WorkspaceContextTray";
import { notify } from "../components/DialogService";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { OpenResearchObject } from "../lib/deskClient";
import { appEvents } from "../lib/appEvents";
import { captureSourceContext } from "../lib/researchContext";

export type FileDropState = "idle" | "over" | "importing";
const plural = (count: number, noun: string) =>
  `${count} ${noun}${count === 1 ? "" : "s"}`;

/**
 * Files dropped on the conversation are imported into its project through
 * the same path as the attach menu, then attached as sources. Files dragged
 * from the project's own file list are already there, so they are attached
 * as they are.
 */
export default function useComposerFileDrop({
  targetRef,
  workspaceId,
  projectId,
  sessionId,
  blocked,
  onBusy,
  onError,
  onNeedsProject,
}: {
  targetRef: RefObject<HTMLElement | null>;
  workspaceId: string | null | undefined;
  /** The project open beside the conversation, whose files can be dragged in. */
  projectId?: string | null;
  sessionId: string | null;
  /** A turn or another change is in progress; drops are declined. */
  blocked: boolean;
  onBusy: (busy: boolean) => void;
  onError: (message: string) => void;
  onNeedsProject?: (paths: string[]) => void;
}): FileDropState {
  const [state, setState] = useState<FileDropState>("idle");
  const latest = useRef({
    workspaceId,
    projectId,
    sessionId,
    blocked,
    onBusy,
    onError,
    onNeedsProject,
  });
  latest.current = {
    workspaceId,
    projectId,
    sessionId,
    blocked,
    onBusy,
    onError,
    onNeedsProject,
  };
  const importing = useRef(false);
  const inside = (x: number, y: number) => {
    const target = targetRef.current;
    const hit = document.elementFromPoint?.(x, y);
    return Boolean(target && hit && target.contains(hit));
  };
  useEffect(() => {
    let disposed = false;
    const attach = async (workspaceId: string, paths: string[]) => {
      const { sessionId, blocked, onBusy, onError } = latest.current;
      if (blocked || importing.current) {
        notify("Wait for the current response or change to finish.", "info");
        return;
      }
      importing.current = true;
      setState("importing");
      onBusy(true);
      let objects: OpenResearchObject[] = [];
      try {
        const captured = await Promise.all(
          paths.map((path) =>
            captureSourceContext(workspaceId, {
              path,
              expectedHash: null,
              checkpointId: null,
              selection: null,
            }),
          ),
        );
        objects = captured.flat().map((item) => item.object);
      } catch (cause) {
        onError(workbenchErrorMessage(cause));
      } finally {
        importing.current = false;
        onBusy(false);
        if (!disposed) setState("idle");
      }
      // The conversation may have changed while the files were read.
      const current = latest.current;
      if (
        objects.length &&
        current.projectId === workspaceId &&
        current.sessionId === sessionId
      )
        addContextObjectsWhenReady(workspaceId, objects);
    };
    const stop = appEvents.on("project-file-drag", (drag) => {
      if (importing.current) return;
      const hit =
        drag.phase !== "cancel" &&
        drag.workspaceId === latest.current.projectId &&
        inside(drag.x, drag.y);
      if (drag.phase === "move") {
        setState(hit ? "over" : "idle");
        return;
      }
      setState("idle");
      if (hit && drag.paths.length) void attach(drag.workspaceId, drag.paths);
    });
    return () => {
      disposed = true;
      stop();
    };
  }, [targetRef]);
  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | null = null;
    const receive = async (paths: string[]) => {
      const {
        workspaceId,
        sessionId,
        blocked,
        onBusy,
        onError,
        onNeedsProject,
      } = latest.current;
      if (blocked || importing.current) {
        notify("Wait for the current response or change to finish.", "info");
        return;
      }
      if (!workspaceId) {
        if (onNeedsProject) {
          onNeedsProject(paths);
          return;
        }
        notify(
          "Files are saved in projects. Open or create a project, then add files there.",
          "info",
        );
        return;
      }
      importing.current = true;
      setState("importing");
      onBusy(true);
      let sources: OpenResearchObject[] = [];
      let unreadable = 0;
      try {
        const { imported, failures } = await importResearchFiles(
          workspaceId,
          paths,
        );
        sources = imported
          .map(paperSource)
          .filter((source): source is OpenResearchObject => source !== null);
        unreadable = imported.length - sources.length;
        if (failures.length) onError(failures.join("\n"));
      } catch (cause) {
        onError(workbenchErrorMessage(cause));
      } finally {
        importing.current = false;
        onBusy(false);
        if (!disposed) setState("idle");
      }
      // The conversation may have changed while files were being read.
      const current = latest.current;
      const attach =
        sources.length > 0 &&
        current.workspaceId === workspaceId &&
        current.sessionId === sessionId;
      if (attach) addContextObjectsWhenReady(workspaceId, sources);
      if (sources.length)
        notify(
          attach
            ? `Added ${plural(sources.length, "file")} to this conversation.`
            : `Added ${plural(sources.length, "file")} to the project.`,
          "success",
        );
      if (unreadable)
        notify(
          `${plural(unreadable, "file")} saved to the project, but the text could not be read.`,
          "info",
        );
    };
    void onFileDrop((event) => {
      if (disposed) return;
      if (event.type === "leave") {
        if (!importing.current) setState("idle");
        return;
      }
      const hit = inside(event.x, event.y);
      if (event.type === "over") {
        if (!importing.current) setState(hit ? "over" : "idle");
        return;
      }
      if (!importing.current) setState("idle");
      if (hit && event.paths.length) void receive(event.paths);
    }).then((stop) => {
      if (disposed) stop();
      else unsubscribe = stop;
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [targetRef]);
  return state;
}
