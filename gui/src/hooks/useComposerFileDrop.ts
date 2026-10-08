import { useEffect, useRef, useState, type RefObject } from "react";
import { onFileDrop } from "../lib/fileDrop";
import { importResearchFiles, paperSource } from "../lib/workspaceAttachments";
import { addContextObjectsWhenReady } from "../components/WorkspaceContextTray";
import { notify } from "../components/DialogService";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { OpenResearchObject } from "../lib/deskClient";

export type FileDropState = "idle" | "over" | "importing";
const plural = (count: number, noun: string) =>
  `${count} ${noun}${count === 1 ? "" : "s"}`;

/**
 * Files dropped on the conversation are imported into its project through
 * the same path as the attach menu, then attached as sources.
 */
export default function useComposerFileDrop({
  targetRef,
  workspaceId,
  sessionId,
  blocked,
  onBusy,
  onError,
}: {
  targetRef: RefObject<HTMLElement | null>;
  workspaceId: string | null | undefined;
  sessionId: string | null;
  /** A turn or another change is in progress; drops are declined. */
  blocked: boolean;
  onBusy: (busy: boolean) => void;
  onError: (message: string) => void;
}): FileDropState {
  const [state, setState] = useState<FileDropState>("idle");
  const latest = useRef({ workspaceId, sessionId, blocked, onBusy, onError });
  latest.current = { workspaceId, sessionId, blocked, onBusy, onError };
  const importing = useRef(false);
  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | null = null;
    const inside = (x: number, y: number) => {
      const target = targetRef.current;
      const hit = document.elementFromPoint?.(x, y);
      return Boolean(target && hit && target.contains(hit));
    };
    const receive = async (paths: string[]) => {
      const { workspaceId, sessionId, blocked, onBusy, onError } =
        latest.current;
      if (blocked || importing.current) {
        notify("Wait for the current response or change to finish.", "info");
        return;
      }
      if (!workspaceId) {
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
