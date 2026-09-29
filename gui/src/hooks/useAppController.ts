import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
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
import { startupPage } from "../lib/appPreferences";
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
  const [taskSessionId, setTaskSessionId] = useState<string | null>(null);
  const [tasksAttention, setTasksAttention] = useState(false);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [discoveryId, setDiscoveryId] = useState<string | null>(null);
  const [discoveryRequest, setDiscoveryRequest] = useState(0);
  const [page, setPageState] = useState<AppPage>(() =>
    startupPage(localStorage),
  );
  const workspaceSaveRef = useRef<(() => Promise<boolean>) | null>(null);
  const registerWorkspaceSave = useCallback(
    (save: (() => Promise<boolean>) | null) => {
      workspaceSaveRef.current = save;
    },
    [],
  );
  const projectSaveRef = useRef<(() => Promise<boolean>) | null>(null);
  const [workflowDirty, setWorkflowDirty] = useState(false);
  const [settingsDirty, setSettingsDirty] = useState(false);
  const [projectsDirty, setProjectsDirty] = useState(false);
  const unsavedRef = useRef({ workflowDirty, settingsDirty, projectsDirty });
  unsavedRef.current = { workflowDirty, settingsDirty, projectsDirty };
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
    if (nextPage === currentPage) return true;
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
  // Even simple close/back callbacks pass through the same guard. Callers that
  // need to mutate destination state use handleNavigate and wait for approval.
  const setPage = useCallback(
    (next: AppPage) => {
      void confirmLeaveCurrentPage(next).then((allowed) => {
        if (allowed) setPageState(next);
      });
    },
    [confirmLeaveCurrentPage],
  );
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
  const [showActivity, setShowActivity] = useState(false);
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
  const [settingsInitialSection, setSettingsInitialSection] =
    useState<SettingsSection>("general");
  const { recentProjects, projectsLoading } = useRecentProjects(page);
  const [workspaceEntry, setWorkspaceEntry] = useState<{
    surface: "chat" | "project";
    request: number;
  }>({
    surface:
      localStorage.getItem("pipeline.workspace.surface") === "chat"
        ? "chat"
        : "project",
    request: 0,
  });
  const [newProjectRequest, setNewProjectRequest] = useState(0);

  useEffect(() => {
    if (
      page === "workspace" ||
      page === "project-index" ||
      page === "home" ||
      page === "main"
    )
      localStorage.setItem("pipeline.ui.page", page === "main" ? "home" : page);
  }, [page]);

  const [settingsTargetId, setSettingsTargetId] = useState<
    string | undefined
  >();
  const [settingsNavigationKey, setSettingsNavigationKey] = useState(0);
  const [helpInitialSection, setHelpInitialSection] = useState<
    "privacy" | undefined
  >();
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
  // A run id to open in History (e.g. from a batch job's "Open" link).
  const [historyRunId, setHistoryRunId] = useState<string | null>(null);
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
          !unsaved.projectsDirty
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
    if (!(await confirmLeaveCurrentPage("main"))) return;
    if (isRunning) {
      setPageState("main");
      return;
    }
    reset();
    setPaperPath(null);
    setInputSelection(null);
    setRunPreview(null);
    setHistoryRunId(null);
    setHistorySourceSelection(null);
    setPendingRun(null);
    setParallelOverrides(null);
    setActiveRunPlan(null);
    setSelectionKey((key) => key + 1);
    setPageState("main");
  };

  const handleNavigate = async (nextPage: AppPage) => {
    if (!(await confirmLeaveCurrentPage(nextPage))) return false;
    if (nextPage === "tasks") setTasksAttention(false);
    if (page === "settings") void checkDependencies();
    if (nextPage === "settings" && page !== "settings") {
      setSettingsInitialSection("general");
      setSettingsTargetId(undefined);
      setSettingsNavigationKey((key) => key + 1);
    }
    if (nextPage === "history") {
      setHistoryRunId(null);
      setHistorySourceSelection(null);
    }
    if (nextPage === "help") setHelpInitialSection(undefined);
    setPageState(nextPage);
    return true;
  };

  const openHelp = async (section?: "privacy") => {
    if (!(await confirmLeaveCurrentPage("help"))) return;
    setHelpInitialSection(section);
    setPageState("help");
  };

  const openWorkspace = async (surface: "chat" | "project") => {
    if (!(await handleNavigate("workspace"))) return;
    setWorkspaceEntry((entry) => ({ surface, request: entry.request + 1 }));
    localStorage.setItem("pipeline.workspace.surface", surface);
  };

  const openWorkspaceProject = async (
    projectId: string,
    destination: WorkspaceDestination = "overview",
  ) => {
    const alreadyWorkspace = pageRef.current === "workspace";
    if (!(await handleNavigate("workspace"))) return;
    saveDeskLayout(projectId, {
      ...loadDeskLayout(projectId),
      tab: destination,
      object: null,
      comparison: null,
    });
    localStorage.setItem("pipeline.workspace.workspaceId", projectId);
    localStorage.setItem("pipeline.workspace.surface", "project");
    setWorkspaceEntry((entry) => ({
      surface: "project",
      request: entry.request + 1,
    }));
    if (alreadyWorkspace) {
      window.dispatchEvent(
        new CustomEvent("pipeline:open-project", { detail: projectId }),
      );
      window.dispatchEvent(
        new CustomEvent("pipeline:research-destination", {
          detail: { workspaceId: projectId, tab: destination },
        }),
      );
    }
  };

  const createWorkspaceProject = async () => {
    if (!(await handleNavigate("workspace"))) return;
    setWorkspaceEntry((entry) => ({
      surface: "project",
      request: entry.request + 1,
    }));
    setNewProjectRequest((request) => request + 1);
  };

  const openPaddleInstallSettings = async () => {
    if (!(await confirmLeaveCurrentPage("settings"))) return;
    setSettingsInitialSection("extraction");
    setSettingsTargetId("paddleocr-local-engine");
    setSettingsNavigationKey((key) => key + 1);
    setPageState("settings");
  };

  const openWorkspaceSession = async (id: string) => {
    const { workbenchClient } = await import("../lib/workbenchClient");
    const snapshot = await workbenchClient.sessionSnapshot(id);
    if (!(await handleNavigate("workspace"))) return false;
    localStorage.setItem("pipeline.workspace.sessionId", id);
    if (snapshot.session.workspaceId)
      localStorage.setItem(
        "pipeline.workspace.workspaceId",
        snapshot.session.workspaceId,
      );
    else localStorage.removeItem("pipeline.workspace.workspaceId");
    localStorage.setItem("pipeline.workspace.surface", "chat");
    setWorkspaceEntry((entry) => ({
      surface: "chat",
      request: entry.request + 1,
    }));
    setShowActivity(false);
    window.dispatchEvent(
      new CustomEvent("pipeline:open-session", { detail: id }),
    );
    return true;
  };

  useEffect(() => {
    const open = (event: Event) => {
      const id = (event as CustomEvent<{ id: string }>).detail?.id;
      if (!id) return;
      void confirmLeaveCurrentPage("tasks").then((allowed) => {
        if (!allowed) return;
        setDiscoveryId(id);
        setDiscoveryRequest((request) => request + 1);
        setTaskSessionId(null);
        setTaskId(null);
        setPageState("tasks");
      });
    };
    window.addEventListener("pipeline:open-discovery", open);
    return () => window.removeEventListener("pipeline:open-discovery", open);
  }, [confirmLeaveCurrentPage]);
  const openTask = async (sessionId: string | null, id?: string) => {
    if (!(await handleNavigate("tasks"))) return;
    setDiscoveryId(null);
    setTaskSessionId(sessionId);
    setTaskId(id ?? null);
  };

  const openWorkspaceSettings = async () => {
    if (!(await confirmLeaveCurrentPage("settings"))) return;
    setSettingsInitialSection("workspace");
    setSettingsTargetId(undefined);
    setSettingsNavigationKey((key) => key + 1);
    setPageState("settings");
  };

  const openHistoryRun = async (
    runId: string,
    source: ArtifactSelectionTarget | null,
  ) => {
    if (!(await handleNavigate("history"))) return;
    setHistoryRunId(runId);
    setHistorySourceSelection(source);
  };

  const closePipeline = async () => {
    if (!(await confirmLeaveCurrentPage("main"))) return;
    setWorkflowDirty(false);
    setPageState("main");
    setConfigVersion((version) => version + 1);
  };

  const inspectResearchAttention = async () => {
    const notice = researchNotice;
    if (!notice || !(await handleNavigate("workspace"))) return;
    const id = notice.workspaceId;
    saveDeskLayout(id, { ...loadDeskLayout(id), tab: "checks" });
    localStorage.setItem("pipeline.workspace.workspaceId", id);
    localStorage.setItem("pipeline.workspace.surface", "project");
    setResearchNotice(null);
    window.dispatchEvent(
      new CustomEvent("pipeline:open-project", { detail: id }),
    );
    window.dispatchEvent(
      new CustomEvent("pipeline:research-destination", {
        detail: { workspaceId: id, tab: "checks" },
      }),
    );
  };

  const openActivityProject = async (id: string) => {
    if (!(await handleNavigate("workspace"))) return;
    saveDeskLayout(id, { ...loadDeskLayout(id), tab: "plans" });
    localStorage.setItem("pipeline.workspace.workspaceId", id);
    localStorage.setItem("pipeline.workspace.surface", "project");
    setShowActivity(false);
    window.dispatchEvent(
      new CustomEvent("pipeline:open-project", { detail: id }),
    );
    window.dispatchEvent(
      new CustomEvent("pipeline:research-destination", {
        detail: { workspaceId: id, tab: "plans" },
      }),
    );
  };

  const openActivityTask = async (id: string) => {
    if (!(await handleNavigate("tasks"))) return;
    setTaskId(id);
    setShowActivity(false);
  };

  const openActivityReview = async (id: string) => {
    const current = "runId" in state && state.runId === id;
    if (!(await handleNavigate(current ? "main" : "history"))) return;
    if (!current) {
      setHistoryRunId(id);
      setHistorySourceSelection(null);
    }
    setShowActivity(false);
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
    setSettingsInitialSection,
    setSettingsTargetId,
    setSettingsNavigationKey,
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
    showActivity,
    setShowActivity,
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
    discoveryRequest,
    closePipeline,
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
    setHistoryRunId,
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
