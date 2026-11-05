import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import {
  projectClient,
  type HomeSettings,
  type ProjectAction,
  type ProjectHome,
} from "../../lib/projectClient";
import { workspaceClient } from "../../lib/workbench/workspaceClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type { Workspace } from "../../lib/workbenchTypes";
import { listen } from "@tauri-apps/api/event";
import { appEvents } from "../../lib/appEvents";

export function useProjectSurfaceData(
  workspaceId: string,
  setRevisionId: Dispatch<SetStateAction<string | null>>,
) {
  const [data, setData] = useState<ProjectHome | null>(null);
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const background = useRef<Promise<void> | null>(null);
  const [watch, setWatch] = useState(false);
  // Every failure shown in this banner passes through here, so a caught error
  // of any shape reaches the researcher as text rather than a raw object.
  const reportError = useCallback(
    (cause: unknown) => setError(workbenchErrorMessage(cause)),
    [],
  );
  const clearError = useCallback(() => setError(null), []);
  const refresh = useCallback(async () => {
    const [home, ws] = await Promise.all([
      projectClient.home(workspaceId),
      workspaceClient.getWorkspace(workspaceId),
    ]);
    setData(home);
    setWorkspace(ws);
    setRevisionId(
      (current) =>
        current ??
        home.settings.body.manuscriptRevisionId ??
        home.papers[0]?.revision?.id ??
        null,
    );
  }, [workspaceId]);
  useEffect(() => {
    void refresh().catch((e) => reportError(workbenchErrorMessage(e)));
  }, [refresh, reportError]);
  // A chat can create and capture its copy while this surface remains mounted.
  // Refresh saved review data without rescanning or modifying the project folder.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let live = true;
    const reload = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        if (!live || busyRef.current || background.current) return;
        background.current = refresh()
          .catch((e) => reportError(workbenchErrorMessage(e)))
          .finally(() => {
            background.current = null;
          });
      }, 100);
    };
    const offNavigation = appEvents.on("workspace-destination", (event) => {
      if (event.workspaceId === workspaceId && event.tab === "edits") reload();
    });
    const offTurn = listen<{ kind: string }>(
      "workbench:event",
      ({ payload }) => {
        if (payload.kind === "turnCompleted") reload();
      },
    ).catch(() => () => {});
    return () => {
      live = false;
      clearTimeout(timer);
      offNavigation();
      void offTurn.then((off) => off()).catch(() => {});
    };
  }, [workspaceId, refresh, reportError]);
  const run = useCallback(
    async (operation: () => Promise<unknown>) => {
      if (busyRef.current) return;
      busyRef.current = true;
      setBusy(true);
      setError(null);
      try {
        // A user action waits for a quiet folder check rather than racing it.
        if (background.current) await background.current;
        await operation();
      } catch (e) {
        setError(workbenchErrorMessage(e));
      } finally {
        try {
          await refresh();
        } catch (e) {
          setError(workbenchErrorMessage(e));
        }
        busyRef.current = false;
        setBusy(false);
      }
    },
    [refresh],
  );
  // An embedded view wrote something that other views show. Queue behind any
  // quiet check, so an older answer cannot replace the newer one.
  const changed = useCallback(() => {
    const reloaded: Promise<void> = (background.current ?? Promise.resolve())
      .then(() => refresh())
      .catch((e) => reportError(e))
      .finally(() => {
        if (background.current === reloaded) background.current = null;
      });
    background.current = reloaded;
  }, [refresh, reportError]);
  const act = useCallback(
    async (action: ProjectAction) =>
      run(() => projectClient.mutate(workspaceId, action)),
    [run, workspaceId],
  );
  const saveSettings = useCallback(
    (patch: Partial<HomeSettings>) =>
      data
        ? act({
            action: "saveHome",
            expectedRevision: data.settings.revision,
            settings: { ...data.settings.body, ...patch },
          })
        : Promise.resolve(),
    [act, data],
  );

  // Focus and timer checks are not user actions: they never disable the surface,
  // show "Saving…", or clear an error the researcher is reading.
  const backgroundRefresh = useCallback(() => {
    if (busyRef.current || background.current) return;
    background.current = projectClient
      .mutate(workspaceId, { action: "checkForChanges" })
      .then(() => refresh())
      .catch((e) => {
        // Something else holds the project, such as a Git action. The next
        // check finds whatever it changed.
        if ((e as { code?: unknown } | null)?.code === "storage_conflict")
          return;
        const message = workbenchErrorMessage(e);
        setError((current) => current ?? message);
      })
      .finally(() => {
        background.current = null;
      });
  }, [refresh, workspaceId]);

  // Bounded polling is optional; a focus refresh handles the usual external-editor
  // round trip. Neither mechanism changes captured document revisions or selection.
  useEffect(() => {
    if (!workspace?.root) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const focus = () => {
      clearTimeout(timer);
      timer = setTimeout(backgroundRefresh, 400);
    };
    window.addEventListener("focus", focus);
    const interval = watch
      ? setInterval(() => {
          if (document.visibilityState === "visible") focus();
        }, 30_000)
      : undefined;
    return () => {
      window.removeEventListener("focus", focus);
      clearTimeout(timer);
      if (interval) clearInterval(interval);
    };
  }, [backgroundRefresh, watch, workspace?.root]);

  return {
    data,
    setData,
    workspace,
    error,
    busy,
    watch,
    setWatch,
    reportError,
    clearError,
    refresh,
    changed,
    run,
    act,
    saveSettings,
  };
}
export type ProjectSurfaceData = ReturnType<typeof useProjectSurfaceData>;
