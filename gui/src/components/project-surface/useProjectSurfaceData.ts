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

export function useProjectSurfaceData(
  workspaceId: string,
  setRevisionId: Dispatch<SetStateAction<string | null>>,
) {
  const [data, setData] = useState<ProjectHome | null>(null);
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [watch, setWatch] = useState(false);
  const reportError = useCallback((message: string) => setError(message), []);
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
  const run = useCallback(
    async (operation: () => Promise<unknown>) => {
      if (busyRef.current) return;
      busyRef.current = true;
      setBusy(true);
      setError(null);
      try {
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

  // Bounded polling is optional; a focus refresh handles the usual external-editor
  // round trip. Neither mechanism changes captured document revisions or selection.
  useEffect(() => {
    if (!workspace?.root) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const focus = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        if (!busyRef.current) void act({ action: "refresh" });
      }, 400);
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
  }, [act, watch, workspace?.root]);

  return {
    data,
    setData,
    workspace,
    error,
    busy,
    watch,
    setWatch,
    reportError,
    refresh,
    run,
    act,
    saveSettings,
  };
}
export type ProjectSurfaceData = ReturnType<typeof useProjectSurfaceData>;
