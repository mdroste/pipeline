import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type Dispatch,
  type SetStateAction,
} from "react";
import { confirmDialog } from "../../components/DialogService";
import type { AppPage } from "../../components/NavRail";
import type { SettingsSection } from "../../components/SettingsPage";
import { startupRoute } from "../../lib/appPreferences";
import { router, type AppRoute } from "../../lib/router";
import type { ArtifactSelectionTarget } from "../../lib/artifactTypes";
import { loadDeskLayout, saveDeskLayout } from "../../lib/deskLayout";
import type { WorkspaceDestination } from "../../lib/workspaceNavigation";
import type { DepsReport } from "../../lib/types";

export function useAppNavigation() {
  // The router owns the current location. Initialize it once per app mount
  // from the persisted route (any destination restores, not just four pages).
  const routerInitialized = useRef(false);
  if (!routerInitialized.current) {
    routerInitialized.current = true;
    router.init(startupRoute(localStorage));
  }
  const routerSnapshot = useSyncExternalStore(
    router.subscribe,
    router.getSnapshot,
  );
  const route = routerSnapshot.route;
  const page = route.page;
  const setPageState = useCallback(
    (next: AppPage) => router.apply({ page: next } as AppRoute),
    [],
  );
  // Route-derived destination parameters.
  const taskSessionId =
    route.page === "tasks" ? (route.taskSessionId ?? null) : null;
  const taskId = route.page === "tasks" ? (route.taskId ?? null) : null;
  const discoveryId =
    route.page === "tasks" ? (route.discoveryId ?? null) : null;
  const automatePath =
    route.page === "tasks" ? (route.automatePath ?? null) : null;
  const [taskEntryRequest, setTaskEntryRequest] = useState(0);
  const workspaceSaveRef = useRef<(() => Promise<boolean>) | null>(null);
  const registerWorkspaceSave = useCallback(
    (save: (() => Promise<boolean>) | null) => {
      workspaceSaveRef.current = save;
    },
    [],
  );
  const projectSaveRef = useRef<(() => Promise<boolean>) | null>(null);
  const [tasksDirty, setTasksDirty] = useState(false);
  const tasksDirtyRef = useRef(false);
  tasksDirtyRef.current = tasksDirty;
  const [workflowDirty, setWorkflowDirty] = useState(false);
  const [settingsDirty, setSettingsDirty] = useState(false);
  const [projectsDirty, setProjectsDirty] = useState(false);
  const unsavedRef = useRef({
    workflowDirty,
    settingsDirty,
    projectsDirty,
    tasksDirty,
  });
  unsavedRef.current = {
    workflowDirty,
    settingsDirty,
    projectsDirty,
    tasksDirty,
  };
  const pageRef = useRef(page);
  pageRef.current = page;
  const navigationSequence = useRef(0);
  const registerProjectSave = useCallback(
    (save: (() => Promise<boolean>) | null) => {
      projectSaveRef.current = save;
    },
    [],
  );
  const confirmLeaveCurrentPage = useCallback(async (nextPage: AppPage) => {
    const sequence = ++navigationSequence.current;
    const currentPage = pageRef.current;
    if (currentPage === "tasks" && tasksDirtyRef.current) {
      const allowed = await confirmDialog(
        "You have unsaved automation changes. Leave and discard them?",
        { confirmLabel: "Discard and leave", destructive: true },
      );
      return allowed && sequence === navigationSequence.current;
    }
    if (nextPage === currentPage && currentPage !== "workspace") return true;
    if (currentPage === "workspace" && workspaceSaveRef.current) {
      const saved = await workspaceSaveRef.current();
      return saved && sequence === navigationSequence.current;
    }
    if (currentPage === "projects" && projectSaveRef.current) {
      const saved = await projectSaveRef.current();
      return saved && sequence === navigationSequence.current;
    }
    const unsaved = unsavedRef.current;
    if (currentPage === "pipeline" && unsaved.workflowDirty) {
      const allowed = await confirmDialog(
        "You have unsaved workflow changes. Leave and discard them?",
        {
          confirmLabel: "Discard and leave",
          destructive: true,
        },
      );
      return allowed && sequence === navigationSequence.current;
    }
    if (currentPage === "settings" && unsaved.settingsDirty) {
      const allowed = await confirmDialog(
        "You have unsaved settings changes. Leave and discard them?",
        {
          confirmLabel: "Discard and leave",
          destructive: true,
        },
      );
      return allowed && sequence === navigationSequence.current;
    }
    return true;
  }, []);
  // Every navigation channel — rail clicks, close/back callbacks, hash
  // back/forward, notification deep links — runs the same leave guard, which
  // the router applies before committing a route.
  useEffect(() => {
    router.setGuard((next) => confirmLeaveCurrentPage(next.page));
    return () => router.setGuard(null);
  }, [confirmLeaveCurrentPage]);
  useEffect(() => {
    router.installHashSync();
  }, []);
  const setPage = useCallback((next: AppPage) => {
    void router.navigate({ page: next } as AppRoute);
  }, []);
  const settingsInitialSection: SettingsSection =
    route.page === "settings" && route.section
      ? (route.section as SettingsSection)
      : "general";
  const settingsTargetId =
    route.page === "settings" ? route.targetId : undefined;
  const helpInitialSection = route.page === "help" ? route.section : undefined;
  const historyRunId = route.page === "history" ? (route.runId ?? null) : null;
  const [workspaceEntry, setWorkspaceEntry] = useState<{
    surface: "chat" | "project";
    target?: Extract<AppRoute, { page: "workspace" }>;
    request: number;
  }>({
    target: route.page === "workspace" ? route : undefined,
    surface:
      localStorage.getItem("pipeline.workspace.surface") === "chat"
        ? "chat"
        : "project",
    request: 0,
  });
  const [newProjectRequest, setNewProjectRequest] = useState(0);

  const [settingsNavigationKey, setSettingsNavigationKey] = useState(0);
  // Rich artifact target accompanying a history run route (not serialized;
  // restored relaunches open the run without a preselected source). It
  // belongs to one run, so a target left from another run never opens.
  const [historySource, setHistorySource] = useState<{
    runId: string;
    target: ArtifactSelectionTarget;
  } | null>(null);
  const historySourceRef = useRef(historySource);
  historySourceRef.current = historySource;
  const historySourceSelection =
    historySource && historySource.runId === historyRunId
      ? historySource.target
      : null;
  const clearHistorySourceSelection = useCallback(
    () => setHistorySource(null),
    [],
  );
  const handleNavigate = async (nextPage: AppPage) => {
    return router.navigate({ page: nextPage } as AppRoute);
  };

  const openHelp = async (section?: "privacy") => {
    await router.navigate({ page: "help", section });
  };

  const openWorkspace = async (surface: "chat" | "project") => {
    await router.navigate({ page: "workspace", surface });
  };

  const openWorkspaceProject = async (
    projectId: string,
    destination: WorkspaceDestination = "overview",
  ) => {
    await router.navigate({
      page: "workspace",
      projectId,
      surface: "project",
      destination,
    });
  };

  const createWorkspaceProject = async () => {
    if (!(await router.navigate({ page: "workspace", surface: "project" })))
      return;
    setNewProjectRequest((request) => request + 1);
  };

  const openPaddleInstallSettings = async () => {
    await router.navigate({
      page: "settings",
      section: "extraction",
      targetId: "paddleocr-local-engine",
    });
  };

  const openWorkspaceSession = async (id: string) => {
    const sequence = ++navigationSequence.current;
    const { conversationClient } =
      await import("../../lib/workbench/conversationClient");
    const snapshot = await conversationClient.sessionSnapshot(id);
    if (sequence !== navigationSequence.current) return false;
    const allowed = await router.navigate({
      page: "workspace",
      sessionId: id,
      projectId: snapshot.session.workspaceId ?? undefined,
      surface: "chat",
    });
    if (!allowed) return false;
    return true;
  };

  const openTask = async (sessionId: string | null, id?: string) => {
    await router.navigate({
      page: "tasks",
      taskSessionId: sessionId ?? undefined,
      taskId: id,
    });
  };

  const openWorkspaceSettings = async () => {
    await router.navigate({ page: "settings", section: "workspace" });
  };

  // The source is set before the route commits, so the report opens on it in
  // its first render instead of loading again when the source arrives.
  const openHistoryRun = useCallback(
    async (runId: string, source: ArtifactSelectionTarget | null) => {
      const previous = historySourceRef.current;
      const next = source ? { runId, target: source } : null;
      setHistorySource(next);
      if (await router.navigate({ page: "history", runId })) return;
      // Declined: restore the earlier source unless a newer one replaced it.
      setHistorySource((current) => (current === next ? previous : current));
    },
    [],
  );

  const openActivityProject = async (id: string) => {
    await router.navigate({
      page: "workspace",
      projectId: id,
      surface: "project",
      destination: "plans",
    });
  };

  const openActivityTask = async (id: string) => {
    await router.navigate({ page: "tasks", taskId: id });
  };

  return {
    route,
    page,
    setPageState,
    taskSessionId,
    taskId,
    discoveryId,
    automatePath,
    taskEntryRequest,
    setTaskEntryRequest,
    registerWorkspaceSave,
    registerProjectSave,
    setTasksDirty,
    setWorkflowDirty,
    setSettingsDirty,
    setProjectsDirty,
    unsavedRef,
    confirmLeaveCurrentPage,
    setPage,
    settingsInitialSection,
    settingsTargetId,
    helpInitialSection,
    historyRunId,
    workspaceEntry,
    setWorkspaceEntry,
    newProjectRequest,
    setNewProjectRequest,
    settingsNavigationKey,
    setSettingsNavigationKey,
    historySourceSelection,
    clearHistorySourceSelection,
    handleNavigate,
    openHelp,
    openWorkspace,
    openWorkspaceProject,
    createWorkspaceProject,
    openPaddleInstallSettings,
    openWorkspaceSession,
    openTask,
    openWorkspaceSettings,
    openHistoryRun,
    openActivityProject,
    openActivityTask,
  };
}

export type AppNavigation = ReturnType<typeof useAppNavigation>;

export function useRememberAppPage(page: AppPage) {
  useEffect(() => {
    // Legacy key kept for older builds that only remembered four pages; the
    // router persists the complete route separately.
    if (
      page === "workspace" ||
      page === "project-index" ||
      page === "home" ||
      page === "main"
    )
      try {
        localStorage.setItem(
          "pipeline.ui.page",
          page === "main" ? "home" : page,
        );
      } catch {
        /* Optional navigation preference. */
      }
  }, [page]);
}

export function useAppRouteCommits(
  navigation: AppNavigation,
  checkDependencies: () => Promise<DepsReport | null>,
  setTasksAttention: Dispatch<SetStateAction<boolean>>,
) {
  const {
    route,
    setTaskEntryRequest,
    setSettingsNavigationKey,
    clearHistorySourceSelection,
    setWorkspaceEntry,
  } = navigation;
  // Route-commit side effects. Registered as a plain router subscription (not
  // an effect on the snapshot) so storage preparation runs synchronously
  // before React renders the destination — pages read these keys on mount.
  const previousRouteRef = useRef<AppRoute>(route);
  useEffect(() => {
    return router.subscribe(({ route: next }) => {
      const previous = previousRouteRef.current;
      previousRouteRef.current = next;
      if (next.page === "tasks") {
        setTasksAttention(false);
        setTaskEntryRequest((request) => request + 1);
      }
      if (previous.page === "settings" && next.page !== "settings")
        void checkDependencies();
      if (next.page === "settings") setSettingsNavigationKey((key) => key + 1);
      // A cited source belongs to one visit to a saved run; opening a run
      // any other way (or leaving it) must not reuse it.
      if (next.page !== "history" || !next.runId) clearHistorySourceSelection();
      if (next.page === "workspace") {
        try {
          if (next.projectId) {
            saveDeskLayout(next.projectId, {
              ...loadDeskLayout(next.projectId),
              tab: (next.destination ?? "overview") as WorkspaceDestination,
              object: null,
              comparison: null,
            });
            localStorage.setItem(
              "pipeline.workspace.workspaceId",
              next.projectId,
            );
          } else if (next.sessionId) {
            localStorage.removeItem("pipeline.workspace.workspaceId");
          }
          if (next.sessionId)
            localStorage.setItem(
              "pipeline.workspace.sessionId",
              next.sessionId,
            );
          if (next.surface)
            localStorage.setItem("pipeline.workspace.surface", next.surface);
        } catch {
          /* Entry identity must not depend on optional preference storage. */
        }
        setWorkspaceEntry((entry) => ({
          surface: next.surface ?? entry.surface,
          target: next,
          request: entry.request + 1,
        }));
      }
    });
  }, [checkDependencies]);
}
