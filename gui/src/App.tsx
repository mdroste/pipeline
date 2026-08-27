import {
  lazy,
  Suspense,
  useState,
  useEffect,
  useLayoutEffect,
  useCallback,
  useRef,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import PipelineProgress from "./components/PipelineProgress";
import DepsCheck from "./components/DepsCheck";
import Console from "./components/Console";
import VariablePrompt from "./components/VariablePrompt";
import RunPreview from "./components/RunPreview";
import UpdateBanner from "./components/UpdateBanner";
import NavRail, { type AppPage } from "./components/NavRail";
import RunSetupPanel from "./components/RunSetupPanel";
import ErrorBoundary from "./components/ErrorBoundary";
import { usePipeline, type ProviderLimitNotice } from "./hooks/usePipeline";
import usePersistentPanelWidth from "./hooks/usePersistentPanelWidth";
import { isMac } from "./lib/platform";
import {
  readThemePreference,
  resolveDarkTheme,
  THEME_STORAGE_KEY,
  type ThemePreference,
} from "./lib/theme";
import type {
  DepsReport,
  VarSpec,
  InputSlot,
  PipelineConfig,
  PrimaryInputSelection,
  RunParallelOverrides,
  BatchJob,
} from "./lib/types";
import type { ExecutionPlanStage } from "./lib/pipelineHelpers";
import type { ArtifactSelectionTarget } from "./components/ArtifactExplorer";
import { confirmDialog, notify } from "./components/DialogService";

interface RunProfileSnapshot {
  profileId: string;
  profileConfigSnapshotId: string;
  inputMode: string;
  variables: VarSpec[];
  inputSlots: InputSlot[];
}

interface PendingRun extends RunProfileSnapshot {
  paperPath: string;
  inputSelection: PrimaryInputSelection | null;
}

interface ExecutionPlanEnvelope {
  profileId: string;
  profileConfigSnapshotId: string;
  profileSnapshotId: string;
  configuredInputMode?: string;
  inputMode: string;
  inputInterpretation: string;
  variables: VarSpec[];
  inputSlots: InputSlot[];
  readiness: DepsReport;
  stages: ExecutionPlanStage[];
  parallelAgents?: string[];
  mergeAgent?: string | null;
}

interface PreparedLaunch {
  snapshot: PendingRun;
  variables?: Record<string, string>;
  extraInputs?: Record<string, string>;
  plan: ExecutionPlanEnvelope;
  config: PipelineConfig;
  parallelOverrides: RunParallelOverrides | null;
  batchPaths?: string[];
}

function configForRunPreview(
  config: PipelineConfig,
  agents: string[],
  overrides: RunParallelOverrides | null,
  mergeAgent?: string | null,
): PipelineConfig {
  return {
    ...config,
    merge: mergeAgent
      ? { ...config.merge, agents: [mergeAgent] }
      : config.merge,
    steps: config.steps.map((step) => step.phase === "parallel" && (overrides || step.agents.length === 0)
      ? {
          ...step,
          agents,
          model: overrides ? "" : step.model,
          model_overrides: overrides?.model_overrides ?? step.model_overrides,
          effort: overrides ? "" : step.effort,
          effort_overrides: overrides?.effort_overrides ?? step.effort_overrides,
        }
      : step),
  };
}

const SettingsPage = lazy(() => import("./components/SettingsPage"));
const PipelinePage = lazy(() => import("./components/PipelinePage"));
const AboutPage = lazy(() => import("./components/AboutPage"));
const HistoryPage = lazy(() => import("./components/HistoryPage"));
const ProjectsPage = lazy(() => import("./components/ProjectsPage"));
const WorkflowGalleryPage = lazy(() => import("./components/WorkflowGalleryPage"));
const loadBatchPanel = () => import("./components/BatchPanel");
const BatchPanel = lazy(loadBatchPanel);
const ReportWorkspace = lazy(() => import("./components/ReportWorkspace"));

function plannedInterpretation(selection: PrimaryInputSelection | null): string | null {
  if (!selection) return null;
  if (selection.interpretation !== "batch") {
    return selection.interpretation;
  }
  return selection.selectionKind === "folder" ? "source_tree" : "document";
}

function hasActiveBatch(jobs: BatchJob[]): boolean {
  return jobs.some((job) => job.status === "pending" || job.status === "running");
}

function providerName(provider: string | null): string {
  if (!provider) return "fallback provider";
  if (provider === "claude") return "Claude";
  if (provider === "codex") return "ChatGPT";
  if (provider === "antigravity") return "Antigravity";
  if (provider === "local") return "Local";
  return provider;
}

function ProviderLimitBanner({
  notices,
  onOpenSettings,
}: {
  notices: ProviderLimitNotice[];
  onOpenSettings: () => void;
}) {
  const blocking = [...notices].reverse().find(
    (notice) => notice.status === "exhausted" || notice.status === "fallback_failed",
  );
  const latest = blocking ?? notices[notices.length - 1];
  if (!latest) return null;
  const recovered = notices.filter((notice) => notice.status === "recovered").length;
  const fallback = latest.fallback_provider
    ? `${providerName(latest.fallback_provider)}${latest.fallback_model ? ` (${latest.fallback_model})` : ""}`
    : null;
  const summary = blocking
    ? latest.status === "fallback_failed"
      ? `${providerName(latest.provider)} reached its account usage limit, and ${fallback ?? "the fallback"} also failed.`
      : `${providerName(latest.provider)} reached its account usage limit. No fallback is configured.`
    : latest.status === "recovered"
      ? `${providerName(latest.provider)} reached its account usage limit. Pipeline continued with ${fallback}.`
      : `${providerName(latest.provider)} reached its account usage limit. Switching this call to ${fallback}.`;

  return (
    <div
      role="alert"
      className={`flex shrink-0 items-center gap-3 border-b px-5 py-2.5 text-xs ${
        blocking
          ? "border-red-200 bg-red-50 text-red-800 dark:border-red-900 dark:bg-red-950/45 dark:text-red-300"
          : "border-amber-200 bg-amber-50 text-amber-900 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-300"
      }`}
    >
      <span className="min-w-0 flex-1">
        <span className="font-semibold">Model usage limit reached.</span>{" "}
        {summary}
        {recovered > 1 ? ` ${recovered} calls have used the fallback.` : ""}
      </span>
      <button
        type="button"
        onClick={onOpenSettings}
        className="shrink-0 rounded border border-current/30 px-2 py-1 font-medium hover:bg-white/50 dark:hover:bg-black/20"
      >
        Fallback settings
      </button>
    </div>
  );
}

function App() {
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
  const [inputSelection, setInputSelection] = useState<PrimaryInputSelection | null>(null);
  const [depsReport, setDepsReport] = useState<DepsReport | null>(null);
  const [depsLoading, setDepsLoading] = useState(true);
  const [depsError, setDepsError] = useState<string | null>(null);
  const [page, setPage] = useState<AppPage>("main");
  const [batchActive, setBatchActive] = useState(false);
  const [settingsInitialSection, setSettingsInitialSection] = useState<
    "llm" | "api-keys" | "workflow" | "extraction" | "general"
  >("llm");
  const [settingsTargetId, setSettingsTargetId] = useState<string | undefined>();
  const [settingsNavigationKey, setSettingsNavigationKey] = useState(0);
  const [helpInitialSection, setHelpInitialSection] = useState<"privacy" | undefined>();
  const [configVersion, setConfigVersion] = useState(0);
  const [selectionKey, setSelectionKey] = useState(0);
  const [workflowDirty, setWorkflowDirty] = useState(false);
  const [settingsDirty, setSettingsDirty] = useState(false);
  const [closeProtectionUnavailable, setCloseProtectionUnavailable] = useState(false);
  const unsavedRef = useRef({ workflowDirty, settingsDirty });
  const allowCloseRef = useRef(false);
  unsavedRef.current = { workflowDirty, settingsDirty };
  const [navRailWidth, setNavRailWidth] = usePersistentPanelWidth(
    "pipeline.ui.navRailWidth",
    176,
    152,
    320,
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
  const [runProfileConfigSnapshotId, setRunProfileConfigSnapshotId] =
    useState<string | null>(null);
  const [runConfigLoading, setRunConfigLoading] = useState(true);
  const [runConfigError, setRunConfigError] = useState<string | null>(null);
  const [preparingRun, setPreparingRun] = useState(false);
  const [parallelOverrides, setParallelOverrides] = useState<RunParallelOverrides | null>(null);
  // Variables and extra input slots the active profile declares; both drive
  // the pre-run options modal.
  const [pendingRun, setPendingRun] = useState<PendingRun | null>(null);
  const [runPreview, setRunPreview] = useState<PreparedLaunch | null>(null);
  const [activeRunPlan, setActiveRunPlan] = useState<ExecutionPlanStage[] | null>(null);
  const launchActive = useRef(false);
  const selectedInputRef = useRef({ inputMode, paperPath, inputSelection });
  selectedInputRef.current = { inputMode, paperPath, inputSelection };
  const parallelOverridesRef = useRef(parallelOverrides);
  parallelOverridesRef.current = parallelOverrides;
  // A run id to open in History (e.g. from a batch job's "Open" link).
  const [historyRunId, setHistoryRunId] = useState<string | null>(null);
  const [historySourceSelection, setHistorySourceSelection] = useState<ArtifactSelectionTarget | null>(null);
  const [theme, setTheme] = useState<ThemePreference>(() =>
    readThemePreference(localStorage),
  );
  const [systemIsDark, setSystemIsDark] = useState(() =>
    window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
  const dark = resolveDarkTheme(theme, systemIsDark);
  const themeApplied = useRef(false);

  // Release smoke tests set a private environment variable and wait for this
  // IPC round trip. Normal app launches take the no-op path in Rust.
  useEffect(() => {
    invoke<boolean>("mark_smoke_ready").catch((error) => {
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
        return invoke<BatchJob[]>("get_batch_status").then((jobs) => {
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
        if (!unsaved.workflowDirty && !unsaved.settingsDirty) return;
        event.preventDefault();
        if (await confirmDialog("You have unsaved changes. Quit and discard them?", {
          title: "Quit Pipeline?",
          confirmLabel: "Discard and quit",
          destructive: true,
        })) {
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
    const animate = themeApplied.current && root.classList.contains("dark") !== dark;
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
  const applyRunProfile = useCallback((setup: RunProfileSnapshot): RunProfileSnapshot => {
    const snapshot = {
      ...setup,
      inputMode: setup.inputMode || "document",
      variables: setup.variables ?? [],
      inputSlots: setup.inputSlots ?? [],
    };
    setInputMode(snapshot.inputMode);
    setRunProfileConfigSnapshotId(snapshot.profileConfigSnapshotId);
    return snapshot;
  }, []);

  const loadRunSetup = useCallback(async (
  ): Promise<RunProfileSnapshot | null> => {
    const request = ++runConfigRequest.current;
    setRunConfigLoading(true);
    setRunConfigError(null);
    try {
      const setup = await invoke<RunProfileSnapshot>("get_run_setup");
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

  const checkDependencies = useCallback(async (): Promise<DepsReport | null> => {
    const request = ++dependencyRequest.current;
    const selected = selectedInputRef.current;
    setDepsLoading(true);
    setDepsError(null);
    try {
      const plan = await invoke<ExecutionPlanEnvelope>("get_execution_plan", {
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

  const handleProfileChange = useCallback((config?: PipelineConfig) => {
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
  }, [inputMode]);

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
      const config = await invoke<PipelineConfig>("get_pipeline_config");
      if (snapshot.inputSelection?.interpretation === "batch") {
        let paths = snapshot.inputSelection.paths;
        if (snapshot.inputSelection.selectionKind === "folder") {
          paths = await invoke<string[]>("list_input_files", {
            dir: snapshot.inputSelection.paths[0],
          });
        }
        if (paths.length === 0) {
          throw new Error("No PDF, LaTeX, or Word documents were found in the selected folder.");
        }
        const representativePath = paths[0];
        const plan = await invoke<ExecutionPlanEnvelope>("get_execution_plan", {
          variables: variables ?? null,
          extraInputs: extraInputs ?? null,
          expectedProfileConfigSnapshotId: snapshot.profileConfigSnapshotId,
          diff: false,
          paperPath: representativePath,
          inputInterpretation: "document",
          ...(parallelOverrides ? { runParallelOverrides: parallelOverrides } : {}),
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
      const plan = await invoke<ExecutionPlanEnvelope>("get_execution_plan", {
        variables: variables ?? null,
        extraInputs: extraInputs ?? null,
        expectedProfileConfigSnapshotId: snapshot.profileConfigSnapshotId,
        diff: false,
        paperPath: snapshot.paperPath,
        inputInterpretation: snapshot.inputSelection?.interpretation ?? null,
        ...(parallelOverrides ? { runParallelOverrides: parallelOverrides } : {}),
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
        await invoke("start_batch", {
          paths: prepared.batchPaths,
          variables: prepared.variables ?? null,
          extraInputs: prepared.extraInputs ?? null,
          expectedProfileConfigSnapshotId: prepared.plan.profileConfigSnapshotId,
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
        setRunConfigError("Select an input and declare how Pipeline should use it.");
        return;
      }
      const snapshot = await loadRunSetup();
      if (!snapshot) return;
      if (snapshot.profileConfigSnapshotId !== expectedProfileConfigSnapshotId) {
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

  const confirmLeaveCurrentPage = async (nextPage: AppPage) => {
    if (nextPage === page) return true;
    if (page === "pipeline" && workflowDirty) {
      return confirmDialog("You have unsaved workflow changes. Leave and discard them?", {
        confirmLabel: "Discard and leave",
        destructive: true,
      });
    }
    if (page === "settings" && settingsDirty) {
      return confirmDialog("You have unsaved settings changes. Leave and discard them?", {
        confirmLabel: "Discard and leave",
        destructive: true,
      });
    }
    return true;
  };

  const handleNewRun = async () => {
    if (!(await confirmLeaveCurrentPage("main"))) return;
    if (isRunning) {
      setPage("main");
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
    setPage("main");
  };

  const handleNavigate = async (nextPage: AppPage) => {
    if (!(await confirmLeaveCurrentPage(nextPage))) return;
    if (page === "settings") void checkDependencies();
    if (nextPage === "settings" && page !== "settings") {
      setSettingsInitialSection("llm");
      setSettingsTargetId(undefined);
      setSettingsNavigationKey((key) => key + 1);
    }
    if (nextPage === "history") {
      setHistoryRunId(null);
      setHistorySourceSelection(null);
    }
    if (nextPage === "help") setHelpInitialSection(undefined);
    setPage(nextPage);
  };

  const openPaddleInstallSettings = async () => {
    if (!(await confirmLeaveCurrentPage("settings"))) return;
    setSettingsInitialSection("extraction");
    setSettingsTargetId("paddleocr-local-engine");
    setSettingsNavigationKey((key) => key + 1);
    setPage("settings");
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

  return (
    <div data-testid="app-shell" className="flex h-screen flex-col overflow-hidden bg-gray-50 dark:bg-[#101010]">
      {showDeps && depsReport && (
        <DepsCheck
          report={depsReport}
          onDismiss={() => setShowDeps(false)}
          onOpenPdfSettings={() => void (async () => {
            if (!(await confirmLeaveCurrentPage("settings"))) return;
            setShowDeps(false);
            setSettingsInitialSection("extraction");
            setSettingsTargetId("paddleocr-local-engine");
            setSettingsNavigationKey((key) => key + 1);
            setPage("settings");
          })()}
        />
      )}

      {pendingRun && (
        <VariablePrompt
          variables={pendingRun.variables}
          inputSlots={pendingRun.inputSlots}
          onCancel={() => setPendingRun(null)}
          onSubmit={(values, inputs) => {
            const snapshot = pendingRun;
            setPendingRun(null);
            void launch(snapshot, values, inputs);
          }}
        />
      )}

      {runPreview && (
        <RunPreview
          plan={runPreview.plan}
          config={runPreview.config}
          inputPath={runPreview.snapshot.paperPath}
          batchCount={runPreview.batchPaths?.length ?? 1}
          onCancel={() => setRunPreview(null)}
          onRun={() => void startPreparedLaunch()}
        />
      )}

      {/* Invisible window-wide drag strip along the very top edge (macOS).
          Thin (16px) so it stays above the content panels' own controls,
          which start at 8px padding — grab the top edge anywhere to drag. */}
      {isMac && (
        <div data-tauri-drag-region className="fixed inset-x-0 top-0 z-30 h-4" />
      )}

      {closeProtectionUnavailable && (
        <div
          role="alert"
          className="flex shrink-0 items-center gap-3 border-b border-amber-300 bg-amber-50 px-5 py-2 text-xs text-amber-900
                     dark:border-amber-800 dark:bg-amber-950/60 dark:text-amber-200"
        >
          <span className="flex-1">
            Window-close protection is unavailable. Save workflow and settings changes before quitting.
          </span>
          <button
            type="button"
            onClick={() => setCloseProtectionUnavailable(false)}
            className="rounded px-2 py-1 font-medium hover:bg-amber-100 dark:hover:bg-amber-900"
          >
            Dismiss
          </button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        <NavRail
          activePage={page}
          hasCurrentRun={hasCurrentRun}
          hasActiveBatch={batchActive}
          runInProgress={isRunning}
          isMac={isMac}
          dependenciesReady={depsReport?.ready ?? null}
          dependenciesLoading={depsLoading}
          dependenciesError={depsError}
          width={navRailWidth}
          onResize={setNavRailWidth}
          onNewRun={handleNewRun}
          onNavigate={handleNavigate}
          onDependencies={() => {
            if (depsReport) setShowDeps(true);
            else void checkDependencies();
          }}
        />

        {showRunSetup && (
          <RunSetupPanel
            configVersion={configVersion}
            configError={runConfigError}
            configLoading={runConfigLoading}
            dependenciesError={depsError}
            inputMode={inputMode}
            listenersReady={listenersReady}
            localLlmActive={depsReport?.deps.some((dependency) =>
              dependency.name === "Local LLM server" && dependency.found
            ) ?? false}
            paperPath={paperPath}
            preparingRun={preparingRun}
            parallelOverrides={parallelOverrides}
            selectionKey={selectionKey}
            width={runSetupWidth}
            onConfigureWorkflow={() => setPage("pipeline")}
            onGenerate={handleGenerate}
            onPaperPathChange={(path) => {
              setPaperPath(path);
              if (path) setRunConfigError(null);
            }}
            onInputSelectionChange={(selection) => {
              setInputSelection(selection);
              setPaperPath(selection?.paths[0] ?? null);
              if (selection) setRunConfigError(null);
            }}
            onPrivacyDetails={() => {
              setHelpInitialSection("privacy");
              setPage("help");
            }}
            onParallelOverridesChange={setParallelOverrides}
            onProfileChange={handleProfileChange}
            onRetryConfig={() => void loadRunSetup()}
            onRetryDependencies={() => void checkDependencies()}
            onResize={setRunSetupWidth}
          />
        )}

        <main className="flex min-w-0 flex-1 flex-col">
          <UpdateBanner />
          {page === "main" && providerLimitNotices.length > 0 && (
            <ProviderLimitBanner
              notices={providerLimitNotices}
              onOpenSettings={() => void (async () => {
                if (!(await confirmLeaveCurrentPage("settings"))) return;
                setSettingsInitialSection("llm");
                setSettingsTargetId("usage-limit-fallback");
                setSettingsNavigationKey((key) => key + 1);
                setPage("settings");
              })()}
            />
          )}
          <div className="flex-1 overflow-auto">
            <Suspense
              fallback={(
                <div className="flex items-center justify-center h-full text-gray-400">
                  <div className="animate-spin w-6 h-6 border-2 border-gray-300 border-t-gray-600 rounded-full" />
                </div>
              )}
            >
            {page === "pipeline" ? (
              <PipelinePage
                onClose={() => {
                  setWorkflowDirty(false);
                  setPage("main");
                  setConfigVersion((v) => v + 1);
                }}
                onDirtyChange={setWorkflowDirty}
                onOpenGallery={() => handleNavigate("gallery")}
                onProfileChange={handleProfileChange}
                showBack={false}
              />
            ) : page === "help" ? (
              <AboutPage
                onClose={() => setPage("main")}
                showBack={false}
                onNavigate={handleNavigate}
                onOpenPdfSettings={openPaddleInstallSettings}
                initialSection={helpInitialSection}
              />
            ) : page === "settings" ? (
              <SettingsPage
                onClose={() => setPage("main")}
                onDirtyChange={setSettingsDirty}
                showBack={false}
                theme={theme}
                onThemeChange={handleThemeChange}
                onSystemChange={() => {
                  setConfigVersion((version) => version + 1);
                  void checkDependencies();
                }}
                initialSection={settingsInitialSection}
                targetId={settingsTargetId}
                navigationKey={settingsNavigationKey}
                dependencies={depsReport}
              />
            ) : page === "history" ? (
              <HistoryPage
                onClose={() => setPage("main")}
                showClose={false}
                initialRunId={historyRunId}
                initialSourceSelection={historySourceSelection}
                runInProgress={isRunning}
                onRerun={(runId, onlyFailed) => {
                  if (isRunning) {
                    // rerunPipeline resets the live progress/log state before
                    // the backend guard rejects the second run, so starting it
                    // here would trash the active run's UI.
                    notify(
                      "A report is already being generated. Wait for it to finish or cancel it first.",
                    );
                    return;
                  }
                  setPage("main");
                  setActiveRunPlan(null);
                  rerunPipeline(runId, { onlyFailed });
                }}
              />
            ) : page === "projects" ? (
              <ProjectsPage
                onOpenRun={(runId, source) => {
                  setHistoryRunId(runId);
                  setHistorySourceSelection(source ?? null);
                  setPage("history");
                }}
              />
            ) : page === "gallery" ? (
              <WorkflowGalleryPage onInstalled={handleProfileChange} />
            ) : page === "batch" ? (
              <BatchPanel
                onClose={() => setPage("main")}
                showClose={false}
                onOpenRun={(runId) => {
                  setHistoryRunId(runId);
                  setHistorySourceSelection(null);
                  setPage("history");
                }}
              />
            ) : state.kind === "done" ? (
              // A rendering defect in the finished-run view must not take
              // down the whole app shell with it.
              <ErrorBoundary>
                <ReportWorkspace
                  runId={state.runId}
                  markdown={state.markdown}
                  report={state.report}
                  extractedText={state.extractedText}
                  durationSecs={runStartedAt ? (Date.now() - runStartedAt) / 1000 : null}
                />
              </ErrorBoundary>
            ) : state.kind === "idle" ? (
              <div className="flex items-center justify-center min-h-full px-8 py-16">
                <div className="w-full max-w-3xl">
                  <ol className="grid grid-cols-1 border-y border-gray-200 dark:border-neutral-800 sm:grid-cols-3">
                    <li className="py-5 sm:pr-5">
                      <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-neutral-400">01</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-neutral-200">
                        {inputMode === "none" ? "Start" : "Choose an input"}
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-neutral-400">
                        {inputMode === "none"
                          ? "No source file is required for this workflow."
                          : "Select a document or folder."}
                      </p>
                    </li>
                    <li className="border-t border-gray-200 py-5 sm:border-l sm:border-t-0 sm:px-5 dark:border-neutral-800">
                      <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-neutral-400">02</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-neutral-200">
                        Select a workflow
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-neutral-400">
                        Choose from built-in workflows or customize your own.
                      </p>
                    </li>
                    <li className="border-t border-gray-200 py-5 sm:border-l sm:border-t-0 sm:pl-5 dark:border-neutral-800">
                      <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-neutral-400">03</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-neutral-200">
                        Review
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-neutral-400">
                        Read and save report(s).
                      </p>
                    </li>
                  </ol>
                </div>
              </div>
            ) : (
              <div className="flex h-full items-center justify-center px-8 py-12">
                <div className="w-full max-w-sm">
                  <PipelineProgress
                    state={state}
                    plan={activeRunPlan ?? undefined}
                    stageHistory={stageHistory}
                    runStartedAt={runStartedAt}
                    passTimes={passTimes}
                    reviewRouting={reviewRouting}
                  />
                  {state.kind === "error" ? (
                    <div className="mt-6">
                      <div
                        role="alert"
                        className="rounded-lg border border-red-200 bg-red-50 px-3 py-2.5 text-sm leading-5 text-red-700
                                   dark:border-red-900 dark:bg-red-950/40 dark:text-red-300"
                      >
                        {state.message}
                      </div>
                      <button
                        type="button"
                        onClick={handleNewRun}
                        className="mt-3 w-full rounded-lg border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700
                                   transition-colors hover:bg-white focus-visible:outline-none focus-visible:ring-2
                                   focus-visible:ring-gray-400 dark:border-neutral-700 dark:text-neutral-300 dark:hover:bg-neutral-900"
                      >
                        Start a new report
                      </button>
                    </div>
                  ) : (
                    <>
                      <p className="mt-6 text-center text-xs leading-relaxed text-gray-500 dark:text-neutral-400">
                        This may take 15–60 minutes depending on<br />
                        paper length, number of agents, and LLM load.
                      </p>
                      <button
                        type="button"
                        onClick={cancel}
                        className="mx-auto mt-4 block rounded-lg px-3 py-1.5 text-xs font-medium text-gray-500 transition-colors
                                   hover:bg-red-50 hover:text-red-600 focus-visible:outline-none focus-visible:ring-2
                                   focus-visible:ring-red-300 dark:text-neutral-400 dark:hover:bg-red-950/40 dark:hover:text-red-400"
                      >
                        Cancel report
                      </button>
                    </>
                  )}
                </div>
              </div>
            )}
            </Suspense>
          </div>

          {logs.length > 0 && <Console logs={logs} usage={usage} active={isRunning} />}
        </main>
      </div>
    </div>
  );
}

export default App;
