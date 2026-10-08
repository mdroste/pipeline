import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { pendingRequestIds } from "../lib/workspaceAttention";
import { loadDeskLayout, saveDeskLayout } from "../lib/deskLayout";
import {
  configForRunPreview,
  hasActiveBatch,
  plannedInterpretation,
  type PendingRun,
  type PreparedLaunch,
  type RunProfileSnapshot,
} from "../lib/appRunPreparation";
import { NAV_RAIL_WIDTH, type AppPage } from "../components/NavRail";
import { usePipeline } from "./usePipeline";
import usePersistentPanelWidth from "./usePersistentPanelWidth";
import {
  readThemePreference,
  resolveDarkTheme,
  THEME_STORAGE_KEY,
  type ThemePreference,
} from "../lib/theme";
import type {
  DepsReport,
  PipelineConfig,
  PrimaryInputSelection,
  RunParallelOverrides,
  BatchJob,
} from "../lib/types";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";
import type { ArtifactSelectionTarget } from "../lib/artifactTypes";
import { confirmDialog } from "../components/DialogService";
import type { SettingsSection } from "../components/SettingsPage";
import type { ReviewHandoff, WorkbenchEvent } from "../lib/workbenchTypes";
import { startupRoute } from "../lib/appPreferences";
import { router, type AppRoute } from "../lib/router";
import { appClient } from "../lib/appClient";
import useRecentProjects from "./useRecentProjects";
import type { WorkspaceDestination } from "../lib/workspaceNavigation";

export function useAppController() {
  const {
    state,
    logs,
    usage,
    startPipeline,
    rerunPipeline,
    cancel,
    reset,
    listenersReady,
    runStartedAt,
    passTimes,
    stageHistory,
    reviewRouting,
    providerLimitNotices = [],
  } = usePipeline();
  const [paperPath, setPaperPath] = useState<string | null>(null);
  const [inputSelection, setInputSelection] =
    useState<PrimaryInputSelection | null>(null);
  const [depsReport, setDepsReport] = useState<DepsReport | null>(null);
  const [depsLoading, setDepsLoading] = useState(true);
  const [depsError, setDepsError] = useState<string | null>(null);
  const [tasksAttention, setTasksAttention] = useState(false);
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
  useEffect(() => {
    const changed = () => {
      if (page !== "tasks") setTasksAttention(true);
    };
    const pending = [
      listen("tasks:notice", changed),
      listen("missions:notice", changed),
      listen("discovery:notice", changed),
    ];
    return () => {
      for (const off of pending)
        void off.then((f) => f()).catch(() => undefined);
    };
  }, [page]);
  const [workspaceActive, setWorkspaceActive] = useState(false);
  const [workspaceAttention, setWorkspaceAttention] = useState(false);
  const [researchNotice, setResearchNotice] = useState<{
    workspaceId: string;
    checkId: string;
  } | null>(null);
  useEffect(() => {
    const off = listen<{ workspaceId: string; checkId: string }>(
      "workbench-research-attention",
      ({ payload }) => setResearchNotice(payload),
    );
    return () => {
      void off.then((f) => f());
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unresolved = new Set<string>();
    let bootstrapping = true;
    const observed: WorkbenchEvent[] = [];
    void import("../lib/workbenchClient")
      .then(({ workbenchClient }) => workbenchClient.pendingRequests())
      .then((requests) => {
        if (!disposed) {
          let ids = new Set<string>();
          for (const r of [...requests, ...observed])
            ids = pendingRequestIds(ids, r);
          unresolved = ids;
          bootstrapping = false;
          setWorkspaceAttention(ids.size > 0);
        }
      })
      .catch(() => {
        bootstrapping = false;
      });
    void listen<WorkbenchEvent>("workbench:event", ({ payload }) => {
      if (
        bootstrapping &&
        ["serverRequest", "serverRequestResolved", "connectionClosed"].includes(
          payload.kind,
        )
      )
        observed.push(payload);
      if (payload.kind === "turnStarted") setWorkspaceActive(true);
      if (
        payload.kind === "turnCompleted" ||
        payload.kind === "connectionClosed"
      )
        setWorkspaceActive(false);
      unresolved = pendingRequestIds(unresolved, payload);
      setWorkspaceAttention(unresolved.size > 0);
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  const [batchActive, setBatchActive] = useState(false);
  const settingsInitialSection: SettingsSection =
    route.page === "settings" && route.section
      ? (route.section as SettingsSection)
      : "general";
  const settingsTargetId =
    route.page === "settings" ? route.targetId : undefined;
  const helpInitialSection = route.page === "help" ? route.section : undefined;
  const historyRunId = route.page === "history" ? (route.runId ?? null) : null;
  const { recentProjects, projectsLoading } = useRecentProjects(page);
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

  const [settingsNavigationKey, setSettingsNavigationKey] = useState(0);
  const [configVersion, setConfigVersion] = useState(0);
  const [selectionKey, setSelectionKey] = useState(0);
  const [closeProtectionUnavailable, setCloseProtectionUnavailable] =
    useState(false);
  const allowCloseRef = useRef(false);
  const [navRailWidth, setNavRailWidth] = usePersistentPanelWidth(
    "pipeline.ui.navRailWidth",
    NAV_RAIL_WIDTH.default,
    NAV_RAIL_WIDTH.min,
    NAV_RAIL_WIDTH.max,
  );
  const [runSetupWidth, setRunSetupWidth] = usePersistentPanelWidth(
    "pipeline.ui.runSetupWidth",
    288,
    240,
    440,
  );
  // Active profile's input mode ("document" | "folder" | "none") — refetched
  // whenever the pipeline config may have changed. "none" workflows can run
  // without selecting an input.
  const [inputMode, setInputMode] = useState<string>("document");
  const [runProfileConfigSnapshotId, setRunProfileConfigSnapshotId] = useState<
    string | null
  >(null);
  const [runConfigLoading, setRunConfigLoading] = useState(true);
  const [runConfigError, setRunConfigError] = useState<string | null>(null);
  const [preparingRun, setPreparingRun] = useState(false);
  const [parallelOverrides, setParallelOverrides] =
    useState<RunParallelOverrides | null>(null);
  // Variables and extra input slots the active profile declares; both drive
  // the pre-run options modal.
  const [pendingRun, setPendingRun] = useState<PendingRun | null>(null);
  const [runPreview, setRunPreview] = useState<PreparedLaunch | null>(null);
  const [activeRunPlan, setActiveRunPlan] = useState<
    ExecutionPlanStage[] | null
  >(null);
  const launchActive = useRef(false);
  const selectedInputRef = useRef({ inputMode, paperPath, inputSelection });
  selectedInputRef.current = { inputMode, paperPath, inputSelection };
  const parallelOverridesRef = useRef(parallelOverrides);
  parallelOverridesRef.current = parallelOverrides;
  // Rich artifact target accompanying a history run route (not serialized;
  // restored relaunches open the run without a preselected source).
  const [historySourceSelection, setHistorySourceSelection] =
    useState<ArtifactSelectionTarget | null>(null);
  const [theme, setTheme] = useState<ThemePreference>(() =>
    readThemePreference(localStorage),
  );
  const [systemIsDark, setSystemIsDark] = useState(
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
  const dark = resolveDarkTheme(theme, systemIsDark);
  const themeApplied = useRef(false);

  // Release smoke tests set a private environment variable and wait for this
  // IPC round trip. Normal app launches take the no-op path in Rust.
  useEffect(() => {
    appClient.markSmokeReady().catch((error) => {
      console.warn("Startup readiness signal failed:", error);
    });
  }, []);

  useEffect(() => {
    let live = true;
    let unlisteners: Array<() => void> = [];
    Promise.allSettled([
      listen<BatchJob[]>("batch:progress", (event) => {
        if (live) setBatchActive(hasActiveBatch(event.payload));
      }),
      listen("batch:done", () => {
        if (live) setBatchActive(false);
      }),
    ])
      .then((registrations) => {
        const registered = registrations.flatMap((registration) =>
          registration.status === "fulfilled" ? [registration.value] : [],
        );
        const failed = registrations.find(
          (registration): registration is PromiseRejectedResult =>
            registration.status === "rejected",
        );
        if (failed) {
          registered.forEach((unlisten) => unlisten());
          throw failed.reason;
        }
        if (!live) {
          registered.forEach((unlisten) => unlisten());
          return;
        }
        unlisteners = registered;
        // Register listeners before taking the snapshot so a just-started
        // batch cannot fall into the gap between the read and subscription.
        return appClient.batchStatus().then((jobs) => {
          if (live) setBatchActive(hasActiveBatch(jobs));
        });
      })
      .catch((error) => {
        console.warn("Unable to track batch status:", error);
      });
    return () => {
      live = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onCloseRequested(async (event) => {
        if (allowCloseRef.current) return;
        const unsaved = unsavedRef.current;
        if (
          !unsaved.workflowDirty &&
          !unsaved.settingsDirty &&
          !unsaved.projectsDirty &&
          !unsaved.tasksDirty
        )
          return;
        event.preventDefault();
        if (
          await confirmDialog(
            "You have unsaved changes. Quit and discard them?",
            {
              title: "Quit Pipeline?",
              confirmLabel: "Discard and quit",
              destructive: true,
            },
          )
        ) {
          allowCloseRef.current = true;
          await getCurrentWindow().close();
        }
      })
      .then((stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      })
      .catch((error) => {
        console.warn("Unable to register close protection:", error);
        if (!disposed) setCloseProtectionUnavailable(true);
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const handleThemeChange = useCallback((preference: ThemePreference) => {
    localStorage.setItem(THEME_STORAGE_KEY, preference);
    setTheme(preference);
  }, []);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const animate =
      themeApplied.current && root.classList.contains("dark") !== dark;
    themeApplied.current = true;
    root.classList.toggle("theme-transitioning", animate);
    root.classList.toggle("dark", dark);
    root.style.colorScheme = dark ? "dark" : "light";
    if (!animate) return;

    const timer = setTimeout(() => {
      root.classList.remove("theme-transitioning");
    }, 350);
    return () => {
      clearTimeout(timer);
      root.classList.remove("theme-transitioning");
    };
  }, [dark]);

  // matchMedia is provided by WebKit/WebView2/WebKitGTK, so system appearance
  // changes update the React UI on macOS, Windows, and Linux.
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => setSystemIsDark(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  // Keep native window chrome in step with the web content. Passing null asks
  // Tauri to follow the OS and is supported across desktop platforms.
  useEffect(() => {
    getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .catch((error) => console.warn("Unable to apply window theme:", error));
  }, [theme]);

  const runConfigRequest = useRef(0);
  const dependencyRequest = useRef(0);
  const applyRunProfile = useCallback(
    (setup: RunProfileSnapshot): RunProfileSnapshot => {
      const snapshot = {
        ...setup,
        inputMode: setup.inputMode || "document",
        variables: setup.variables ?? [],
        inputSlots: setup.inputSlots ?? [],
      };
      setInputMode(snapshot.inputMode);
      setRunProfileConfigSnapshotId(snapshot.profileConfigSnapshotId);
      return snapshot;
    },
    [],
  );

  const loadRunSetup =
    useCallback(async (): Promise<RunProfileSnapshot | null> => {
      const request = ++runConfigRequest.current;
      setRunConfigLoading(true);
      setRunConfigError(null);
      try {
        const setup = await appClient.runSetup();
        if (request !== runConfigRequest.current) return null;
        return applyRunProfile(setup);
      } catch (error) {
        if (request !== runConfigRequest.current) return null;
        const message = error instanceof Error ? error.message : String(error);
        setRunConfigError(message);
        return null;
      } finally {
        if (request === runConfigRequest.current) {
          setRunConfigLoading(false);
        }
      }
    }, [applyRunProfile]);

  const checkDependencies =
    useCallback(async (): Promise<DepsReport | null> => {
      const request = ++dependencyRequest.current;
      const selected = selectedInputRef.current;
      setDepsLoading(true);
      setDepsError(null);
      try {
        const plan = await appClient.executionPlan({
          variables: null,
          extraInputs: null,
          expectedProfileConfigSnapshotId: null,
          diff: false,
          paperPath: selected.inputMode === "none" ? "" : selected.paperPath,
          inputInterpretation: plannedInterpretation(selected.inputSelection),
          ...(parallelOverridesRef.current
            ? { runParallelOverrides: parallelOverridesRef.current }
            : {}),
        });
        if (request !== dependencyRequest.current) return null;
        setDepsReport(plan.readiness);
        return plan.readiness;
      } catch (error) {
        if (request !== dependencyRequest.current) return null;
        const message = error instanceof Error ? error.message : String(error);
        setDepsReport(null);
        setDepsError(message);
        return null;
      } finally {
        if (request === dependencyRequest.current) setDepsLoading(false);
      }
    }, []);

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
      if (next.page === "history" && !next.runId)
        setHistorySourceSelection(null);
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

  useEffect(() => {
    void loadRunSetup();
  }, [configVersion, loadRunSetup]);

  const startupDependencyCheckStarted = useRef(false);
  useEffect(() => {
    if (startupDependencyCheckStarted.current) return;
    startupDependencyCheckStarted.current = true;
    void checkDependencies();
  }, [checkDependencies]);

  const handleProfileChange = useCallback(
    (config?: PipelineConfig) => {
      const nextInputMode = config?.extraction.input_mode?.trim() || "document";
      // A workflow mutation invalidates the config fingerprint. Retain the
      // selected primary input only when the authoritative schema still has the
      // same mode; otherwise a file can leak into a folder workflow (or vice
      // versa) and fail only after launch.
      if (!config || nextInputMode !== inputMode) {
        setInputMode(nextInputMode);
        setPaperPath(null);
        setInputSelection(null);
        setPendingRun(null);
        setSelectionKey((key) => key + 1);
      }
      setRunProfileConfigSnapshotId(null);
      setParallelOverrides(null);
      dependencyRequest.current += 1;
      setDepsReport(null);
      setDepsLoading(false);
      setDepsError(null);
      setRunConfigLoading(true);
      setConfigVersion((version) => version + 1);
    },
    [inputMode],
  );

  const foregroundRunActive =
    state.kind !== "idle" && state.kind !== "done" && state.kind !== "error";
  const isRunning = foregroundRunActive || batchActive;
  const hasCurrentRun = state.kind !== "idle";
  const showRunSetup = page === "main" && state.kind === "idle";

  const launch = async (
    snapshot: PendingRun,
    variables?: Record<string, string>,
    extraInputs?: Record<string, string>,
  ) => {
    if (launchActive.current) return;
    launchActive.current = true;
    setPreparingRun(true);
    try {
      // Read the editable profile immediately before asking the backend to
      // bind the immutable execution snapshot. If the active profile changed,
      // the expected fingerprint below rejects the launch rather than showing
      // a preview for mixed configurations.
      const config = await appClient.pipelineConfig();
      if (snapshot.inputSelection?.interpretation === "batch") {
        let paths = snapshot.inputSelection.paths;
        if (snapshot.inputSelection.selectionKind === "folder") {
          paths = await appClient.inputFiles(snapshot.inputSelection.paths[0]);
        }
        if (paths.length === 0) {
          throw new Error(
            "No PDF, LaTeX, or Word documents were found in the selected folder.",
          );
        }
        const representativePath = paths[0];
        const plan = await appClient.executionPlan({
          variables: variables ?? null,
          extraInputs: extraInputs ?? null,
          expectedProfileConfigSnapshotId: snapshot.profileConfigSnapshotId,
          diff: false,
          paperPath: representativePath,
          inputInterpretation: "document",
          ...(parallelOverrides
            ? { runParallelOverrides: parallelOverrides }
            : {}),
        });
        setDepsReport(plan.readiness);
        if (!plan.readiness.ready) {
          setShowDeps(true);
          return;
        }
        setRunPreview({
          snapshot,
          variables,
          extraInputs,
          plan,
          config: configForRunPreview(
            config,
            plan.parallelAgents ?? [],
            parallelOverrides,
            plan.mergeAgent,
          ),
          parallelOverrides,
          batchPaths: paths,
        });
        return;
      }
      const plan = await appClient.executionPlan({
        variables: variables ?? null,
        extraInputs: extraInputs ?? null,
        expectedProfileConfigSnapshotId: snapshot.profileConfigSnapshotId,
        diff: false,
        paperPath: snapshot.paperPath,
        inputInterpretation: snapshot.inputSelection?.interpretation ?? null,
        ...(parallelOverrides
          ? { runParallelOverrides: parallelOverrides }
          : {}),
      });
      setDepsReport(plan.readiness);
      if (!plan.readiness.ready) {
        setShowDeps(true);
        return;
      }
      setRunPreview({
        snapshot,
        variables,
        extraInputs,
        plan,
        config: configForRunPreview(
          config,
          plan.parallelAgents ?? [],
          parallelOverrides,
          plan.mergeAgent,
        ),
        parallelOverrides,
      });
    } catch (error) {
      setRunConfigError(
        `The workflow execution plan could not be prepared: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    } finally {
      launchActive.current = false;
      setPreparingRun(false);
    }
  };

  const startPreparedLaunch = async () => {
    const prepared = runPreview;
    if (!prepared || launchActive.current) return;
    launchActive.current = true;
    setRunPreview(null);
    setPreparingRun(true);
    try {
      if (prepared.batchPaths) {
        await appClient.startBatch({
          paths: prepared.batchPaths,
          variables: prepared.variables ?? null,
          extraInputs: prepared.extraInputs ?? null,
          expectedProfileConfigSnapshotId:
            prepared.plan.profileConfigSnapshotId,
          ...(prepared.parallelOverrides
            ? { runParallelOverrides: prepared.parallelOverrides }
            : {}),
        });
        setBatchActive(true);
        setPage("batch");
        return;
      }
      setPage("main");
      setActiveRunPlan(prepared.plan.stages);
      const launchArgs = [
        prepared.snapshot.paperPath,
        prepared.snapshot.inputSelection?.interpretation,
        false,
        prepared.variables,
        prepared.extraInputs,
        prepared.plan.profileSnapshotId,
      ] as const;
      if (prepared.parallelOverrides) {
        void startPipeline(...launchArgs, prepared.parallelOverrides);
      } else {
        void startPipeline(...launchArgs);
      }
    } catch (error) {
      setRunConfigError(
        `The report could not be started: ${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      launchActive.current = false;
      setPreparingRun(false);
    }
  };

  const handleGenerate = async () => {
    if (preparingRun || runConfigLoading || !runProfileConfigSnapshotId) return;
    setPreparingRun(true);
    try {
      const expectedProfileConfigSnapshotId = runProfileConfigSnapshotId;
      const selectedPath = inputMode === "none" ? "" : (paperPath ?? "");
      if (inputMode !== "none" && !selectedPath) {
        setRunConfigError(
          "The active workflow requires an input. Select it again before running.",
        );
        return;
      }
      if (inputMode !== "none" && !inputSelection) {
        setRunConfigError(
          "Select an input and declare how Pipeline should use it.",
        );
        return;
      }
      const snapshot = await loadRunSetup();
      if (!snapshot) return;
      if (
        snapshot.profileConfigSnapshotId !== expectedProfileConfigSnapshotId
      ) {
        setRunConfigError(
          "The active profile or settings changed while run inputs were being collected. Review the updated setup and try again.",
        );
        return;
      }
      const pending = { ...snapshot, paperPath: selectedPath, inputSelection };
      if (snapshot.variables.length > 0 || snapshot.inputSlots.length > 0) {
        setPendingRun(pending);
        return;
      }
      await launch(pending);
    } finally {
      setPreparingRun(false);
    }
  };

  const handleWorkspaceReviewHandoff = async (handoff: ReviewHandoff) => {
    if (isRunning) {
      setRunConfigError(
        "A Review run is already active. The Workspace conversation remains usable; open this handoff after the Review run finishes.",
      );
      setPage("main");
      return;
    }
    const selection: PrimaryInputSelection = {
      paths: [handoff.stagedPath],
      interpretation: handoff.inputInterpretation,
      selectionKind:
        handoff.inputInterpretation === "source_tree" ? "folder" : "file",
    };
    setPage("main");
    setPaperPath(handoff.stagedPath);
    setInputSelection(selection);
    setRunConfigError(null);
    setPreparingRun(true);
    try {
      const setup = await loadRunSetup();
      if (!setup) return;
      const pending = {
        ...setup,
        paperPath: handoff.stagedPath,
        inputSelection: selection,
      };
      if (setup.variables.length > 0 || setup.inputSlots.length > 0) {
        setPendingRun(pending);
      } else {
        await launch(pending);
      }
    } finally {
      setPreparingRun(false);
    }
  };

  const handleNewRun = async () => {
    if (!(await router.navigate({ page: "main" }))) return;
    if (isRunning) return;
    reset();
    setPaperPath(null);
    setInputSelection(null);
    setRunPreview(null);
    setHistorySourceSelection(null);
    setPendingRun(null);
    setParallelOverrides(null);
    setActiveRunPlan(null);
    setSelectionKey((key) => key + 1);
  };

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
    const { workbenchClient } = await import("../lib/workbenchClient");
    const snapshot = await workbenchClient.sessionSnapshot(id);
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

  const openHistoryRun = async (
    runId: string,
    source: ArtifactSelectionTarget | null,
  ) => {
    if (!(await router.navigate({ page: "history", runId }))) return;
    setHistorySourceSelection(source);
  };

  const closePipeline = async () => {
    if (!(await router.navigate({ page: "main" }))) return;
    setWorkflowDirty(false);
    setConfigVersion((version) => version + 1);
  };

  const inspectResearchAttention = async () => {
    const notice = researchNotice;
    if (!notice) return;
    const allowed = await router.navigate({
      page: "workspace",
      projectId: notice.workspaceId,
      surface: "project",
      destination: "checks",
    });
    if (allowed) setResearchNotice(null);
  };

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

  const openActivityReview = async (id: string) => {
    const current = "runId" in state && state.runId === id;
    const allowed = await router.navigate(
      current ? { page: "main" } : { page: "history", runId: id },
    );
    if (!allowed) return;
    if (!current) setHistorySourceSelection(null);
  };

  const [showDeps, setShowDeps] = useState(false);

  // Auto-show missing dependencies only on the main page. If the user has
  // already navigated (most importantly, to Settings), do not let a late
  // readiness response cover that destination with this modal.
  useEffect(() => {
    if (page === "main" && depsReport && !depsReport.ready) {
      setShowDeps(true);
    }
  }, [depsReport, page]);

  return {
    showDeps,
    depsReport,
    setShowDeps,
    confirmLeaveCurrentPage,
    setPageState,
    pendingRun,
    setPendingRun,
    launch,
    runPreview,
    setRunPreview,
    startPreparedLaunch,
    closeProtectionUnavailable,
    setCloseProtectionUnavailable,
    researchNotice,
    inspectResearchAttention,
    setResearchNotice,
    openWorkspaceSession,
    openActivityProject,
    openActivityTask,
    openActivityReview,
    tasksAttention,
    page,
    hasCurrentRun,
    isRunning,
    workspaceActive,
    workspaceAttention,
    depsLoading,
    depsError,
    navRailWidth,
    setNavRailWidth,
    handleNewRun,
    handleNavigate,
    recentProjects,
    projectsLoading,
    openWorkspaceProject,
    openWorkspace,
    checkDependencies,
    showRunSetup,
    configVersion,
    runConfigError,
    setRunConfigError,
    runConfigLoading,
    inputMode,
    listenersReady,
    paperPath,
    preparingRun,
    parallelOverrides,
    selectionKey,
    runSetupWidth,
    setPaperPath,
    setInputSelection,
    openHelp,
    setParallelOverrides,
    handleProfileChange,
    handleGenerate,
    loadRunSetup,
    setRunSetupWidth,
    providerLimitNotices,
    batchActive,
    createWorkspaceProject,
    setPage,
    workspaceEntry,
    newProjectRequest,
    setNewProjectRequest,
    openTask,
    openWorkspaceSettings,
    handleWorkspaceReviewHandoff,
    taskSessionId,
    taskId,
    discoveryId,
    automatePath,
    taskEntryRequest,
    closePipeline,
    setTasksDirty,
    setWorkflowDirty,
    openPaddleInstallSettings,
    helpInitialSection,
    setSettingsDirty,
    theme,
    handleThemeChange,
    setConfigVersion,
    settingsInitialSection,
    settingsTargetId,
    settingsNavigationKey,
    historyRunId,
    historySourceSelection,
    setHistorySourceSelection,
    openHistoryRun,
    registerProjectSave,
    registerWorkspaceSave,
    setProjectsDirty,
    state,
    setActiveRunPlan,
    rerunPipeline,
    activeRunPlan,
    stageHistory,
    runStartedAt,
    passTimes,
    reviewRouting,
    cancel,
    logs,
    usage,
  };
}
