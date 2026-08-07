import { lazy, Suspense, useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import PipelineProgress from "./components/PipelineProgress";
import DepsCheck from "./components/DepsCheck";
import Console from "./components/Console";
import VariablePrompt from "./components/VariablePrompt";
import UpdateBanner from "./components/UpdateBanner";
import NavRail, { type AppPage } from "./components/NavRail";
import RunSetupPanel from "./components/RunSetupPanel";
import { usePipeline } from "./hooks/usePipeline";
import usePersistentPanelWidth from "./hooks/usePersistentPanelWidth";
import { isMac } from "./lib/platform";
import type { DepsReport, VarSpec, InputSlot, PipelineConfig } from "./lib/types";
import type { ExecutionPlanStage } from "./lib/pipelineHelpers";

interface RunProfileSnapshot {
  profileId: string;
  profileConfigSnapshotId: string;
  inputMode: string;
  variables: VarSpec[];
  inputSlots: InputSlot[];
  readiness: DepsReport;
}

interface PendingRun extends RunProfileSnapshot {
  paperPath: string;
}

interface ExecutionPlanEnvelope {
  profileId: string;
  profileConfigSnapshotId: string;
  profileSnapshotId: string;
  inputMode: string;
  variables: VarSpec[];
  inputSlots: InputSlot[];
  readiness: DepsReport;
  stages: ExecutionPlanStage[];
}

const SettingsPage = lazy(() => import("./components/SettingsPage"));
const PipelinePage = lazy(() => import("./components/PipelinePage"));
const AboutPage = lazy(() => import("./components/AboutPage"));
const HistoryPage = lazy(() => import("./components/HistoryPage"));
const BatchPanel = lazy(() => import("./components/BatchPanel"));
const ReportWorkspace = lazy(() => import("./components/ReportWorkspace"));

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
  } = usePipeline();
  const [paperPath, setPaperPath] = useState<string | null>(null);
  const [depsReport, setDepsReport] = useState<DepsReport | null>(null);
  const [depsLoading, setDepsLoading] = useState(true);
  const [depsError, setDepsError] = useState<string | null>(null);
  const runPreflightActive = useRef(false);
  const [page, setPage] = useState<AppPage>("main");
  const [configVersion, setConfigVersion] = useState(0);
  const [selectionKey, setSelectionKey] = useState(0);
  const [workflowDirty, setWorkflowDirty] = useState(false);
  const [settingsDirty, setSettingsDirty] = useState(false);
  const [closeProtectionUnavailable, setCloseProtectionUnavailable] = useState(false);
  const unsavedRef = useRef({ workflowDirty, settingsDirty });
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
  // Variables and extra input slots the active profile declares; both drive
  // the pre-run options modal.
  const [pendingRun, setPendingRun] = useState<PendingRun | null>(null);
  const [activeRunPlan, setActiveRunPlan] = useState<ExecutionPlanStage[] | null>(null);
  const launchActive = useRef(false);
  const selectedInputRef = useRef({ inputMode, paperPath });
  selectedInputRef.current = { inputMode, paperPath };
  // A run id to open in History (e.g. from a batch job's "Open" link).
  const [historyRunId, setHistoryRunId] = useState<string | null>(null);
  // Theme: explicit choice in Settings is persisted; otherwise follow the OS.
  const [dark, setDark] = useState(() => {
    const stored = localStorage.getItem("theme");
    if (stored === "dark") return true;
    if (stored === "light") return false;
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  });

  // Release smoke tests set a private environment variable and wait for this
  // IPC round trip. Normal app launches take the no-op path in Rust.
  useEffect(() => {
    invoke<boolean>("mark_smoke_ready").catch((error) => {
      console.warn("Startup readiness signal failed:", error);
    });
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onCloseRequested((event) => {
        const unsaved = unsavedRef.current;
        if (!unsaved.workflowDirty && !unsaved.settingsDirty) return;
        if (!window.confirm("You have unsaved changes. Quit and discard them?")) {
          event.preventDefault();
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

  const handleDarkChange = useCallback((v: boolean) => {
    localStorage.setItem("theme", v ? "dark" : "light");
    setDark(v);
  }, []);

  useEffect(() => {
    document.documentElement.classList.add("theme-transitioning");
    document.documentElement.classList.toggle("dark", dark);
    const timer = setTimeout(() => {
      document.documentElement.classList.remove("theme-transitioning");
    }, 350);
    return () => clearTimeout(timer);
  }, [dark]);

  // Follow OS theme changes unless the user set an explicit preference
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => {
      if (!localStorage.getItem("theme")) setDark(e.matches);
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  const runConfigRequest = useRef(0);
  const applyRunProfile = useCallback((plan: ExecutionPlanEnvelope): RunProfileSnapshot => {
    const snapshot = {
      profileId: plan.profileId,
      profileConfigSnapshotId: plan.profileConfigSnapshotId,
      inputMode: plan.inputMode || "document",
      variables: plan.variables ?? [],
      inputSlots: plan.inputSlots ?? [],
      readiness: plan.readiness,
    };
    setInputMode(snapshot.inputMode);
    setRunProfileConfigSnapshotId(snapshot.profileConfigSnapshotId);
    return snapshot;
  }, []);

  const loadRunConfig = useCallback(async (
    expectedProfileConfigSnapshotId?: string | null,
    selectedPaperPath?: string | null,
  ): Promise<RunProfileSnapshot | null> => {
    const request = ++runConfigRequest.current;
    setRunConfigLoading(true);
    setRunConfigError(null);
    setDepsLoading(true);
    setDepsError(null);
    try {
      const plan = await invoke<ExecutionPlanEnvelope>("get_execution_plan", {
        variables: null,
        extraInputs: null,
        expectedProfileConfigSnapshotId: expectedProfileConfigSnapshotId ?? null,
        diff: false,
        paperPath: selectedPaperPath ?? null,
      });
      if (request !== runConfigRequest.current) return null;
      const snapshot = applyRunProfile(plan);
      setDepsReport(plan.readiness);
      return snapshot;
    } catch (error) {
      if (request !== runConfigRequest.current) return null;
      const message = error instanceof Error ? error.message : String(error);
      setRunConfigError(message);
      setDepsReport(null);
      setDepsError(message);
      return null;
    } finally {
      if (request === runConfigRequest.current) {
        setRunConfigLoading(false);
        setDepsLoading(false);
      }
    }
  }, [applyRunProfile]);

  const checkDependencies = useCallback(async (): Promise<DepsReport | null> => {
    const snapshot = await loadRunConfig(
      null,
      inputMode === "none" ? "" : paperPath,
    );
    return snapshot?.readiness ?? null;
  }, [inputMode, loadRunConfig, paperPath]);

  useEffect(() => {
    const selected = selectedInputRef.current;
    void loadRunConfig(
      null,
      selected.inputMode === "none" ? "" : selected.paperPath,
    );
  }, [configVersion, loadRunConfig]);

  const handleProfileChange = useCallback((config?: PipelineConfig) => {
    const nextInputMode = config?.extraction.input_mode?.trim() || "document";
    // A workflow mutation invalidates the config fingerprint. Retain the
    // selected primary input only when the authoritative schema still has the
    // same mode; otherwise a file can leak into a folder workflow (or vice
    // versa) and fail only after launch.
    if (!config || nextInputMode !== inputMode) {
      setInputMode(nextInputMode);
      setPaperPath(null);
      setPendingRun(null);
      setSelectionKey((key) => key + 1);
    }
    setRunProfileConfigSnapshotId(null);
    setRunConfigLoading(true);
    setConfigVersion((version) => version + 1);
  }, [inputMode]);

  const isRunning =
    state.kind !== "idle" && state.kind !== "done" && state.kind !== "error";
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
      const plan = await invoke<ExecutionPlanEnvelope>("get_execution_plan", {
        variables: variables ?? null,
        extraInputs: extraInputs ?? null,
        expectedProfileConfigSnapshotId: snapshot.profileConfigSnapshotId,
        diff: false,
        paperPath: snapshot.paperPath,
      });
      setDepsReport(plan.readiness);
      if (!plan.readiness.ready) {
        setShowDeps(true);
        return;
      }
      setPage("main");
      setActiveRunPlan(plan.stages);
      void startPipeline(
        snapshot.paperPath,
        false,
        variables,
        extraInputs,
        plan.profileSnapshotId,
      );
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

  const handleGenerate = async () => {
    if (
      runPreflightActive.current ||
      preparingRun ||
      depsLoading ||
      depsReport?.ready !== true
    ) return;
    runPreflightActive.current = true;
    setPreparingRun(true);
    try {
      // Re-read immediately before launch. The workflow switcher and editor
      // both change the active profile asynchronously, so the values shown
      // during setup are not authoritative enough for a run snapshot.
      const snapshot = await loadRunConfig(
        runProfileConfigSnapshotId,
        inputMode === "none" ? "" : paperPath,
      );
      if (!snapshot) return;
      if (!snapshot.readiness.ready) {
        setShowDeps(true);
        return;
      }
      const selectedPath = snapshot.inputMode === "none" ? "" : paperPath;
      if (!selectedPath) {
        setRunConfigError(
          "The active workflow requires an input. Select it again before running.",
        );
        return;
      }
      const pending = { ...snapshot, paperPath: selectedPath };
      if (snapshot.variables.length > 0 || snapshot.inputSlots.length > 0) {
        setPendingRun(pending);
        return;
      }
      await launch(pending);
    } finally {
      runPreflightActive.current = false;
      setPreparingRun(false);
    }
  };

  const confirmLeaveCurrentPage = (nextPage: AppPage) => {
    if (nextPage === page) return true;
    if (page === "pipeline" && workflowDirty) {
      return window.confirm("You have unsaved workflow changes. Leave and discard them?");
    }
    if (page === "settings" && settingsDirty) {
      return window.confirm("You have unsaved settings changes. Leave and discard them?");
    }
    return true;
  };

  const handleNewRun = () => {
    if (!confirmLeaveCurrentPage("main")) return;
    if (isRunning) {
      setPage("main");
      return;
    }
    reset();
    setPaperPath(null);
    setHistoryRunId(null);
    setPendingRun(null);
    setActiveRunPlan(null);
    setSelectionKey((key) => key + 1);
    setPage("main");
  };

  const handleNavigate = (nextPage: AppPage) => {
    if (!confirmLeaveCurrentPage(nextPage)) return;
    if (page === "settings") void checkDependencies();
    if (nextPage === "history") setHistoryRunId(null);
    setPage(nextPage);
  };

  const [showDeps, setShowDeps] = useState(false);

  // Auto-show deps modal on startup if required deps are missing
  useEffect(() => {
    if (depsReport && !depsReport.ready) {
      setShowDeps(true);
    }
  }, [depsReport]);

  return (
    <div data-testid="app-shell" className="flex h-screen flex-col overflow-hidden bg-gray-50 dark:bg-gray-950">
      {showDeps && depsReport && (
        <DepsCheck
          report={depsReport}
          onDismiss={() => setShowDeps(false)}
          onRefresh={() => void checkDependencies()}
          refreshing={depsLoading}
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
            dependenciesLoading={depsLoading}
            dependenciesReady={depsReport?.ready === true}
            inputMode={inputMode}
            listenersReady={listenersReady}
            paperPath={paperPath}
            preparingRun={preparingRun}
            selectionKey={selectionKey}
            width={runSetupWidth}
            onConfigureWorkflow={() => setPage("pipeline")}
            onGenerate={handleGenerate}
            onPaperPathChange={(path) => {
              setPaperPath(path);
              if (path) setRunConfigError(null);
              // Readiness depends on the effective input type: .tex/.docx can
              // bypass the PDF extractor required by a conservative startup
              // check. Refresh against the selected path before gating Run.
              void loadRunConfig(
                runProfileConfigSnapshotId,
                inputMode === "none" ? "" : path,
              );
            }}
            onPrivacyDetails={() => setPage("help")}
            onProfileChange={handleProfileChange}
            onRetryConfig={() => void checkDependencies()}
            onRetryDependencies={() => void checkDependencies()}
            onResize={setRunSetupWidth}
          />
        )}

        <main className="flex min-w-0 flex-1 flex-col">
          <UpdateBanner />
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
                onProfileChange={handleProfileChange}
                showBack={false}
              />
            ) : page === "help" ? (
              <AboutPage onClose={() => setPage("main")} showBack={false} />
            ) : page === "settings" ? (
              <SettingsPage
                onClose={() => setPage("main")}
                onDirtyChange={setSettingsDirty}
                showBack={false}
                dark={dark}
                onDarkChange={handleDarkChange}
                onSystemChange={() => void checkDependencies()}
              />
            ) : page === "history" ? (
              <HistoryPage
                onClose={() => setPage("main")}
                showClose={false}
                initialRunId={historyRunId}
                onRerun={(runId, onlyFailed) => {
                  setPage("main");
                  setActiveRunPlan(null);
                  rerunPipeline(runId, { onlyFailed });
                }}
              />
            ) : page === "batch" ? (
              <BatchPanel
                onClose={() => setPage("main")}
                showClose={false}
                onOpenRun={(runId) => { setHistoryRunId(runId); setPage("history"); }}
              />
            ) : state.kind === "done" ? (
              <ReportWorkspace
                runId={state.runId}
                markdown={state.markdown}
                report={state.report}
                extractedText={state.extractedText}
                durationSecs={runStartedAt ? (Date.now() - runStartedAt) / 1000 : null}
              />
            ) : state.kind === "idle" ? (
              <div className="flex items-center justify-center min-h-full px-8 py-16">
                <div className="w-full max-w-3xl">
                  <h1 className="max-w-2xl text-3xl font-semibold tracking-tight text-gray-900 dark:text-gray-100">
                    Pipeline
                  </h1>

                  <ol className="mt-8 grid grid-cols-1 border-y border-gray-200 dark:border-gray-800 sm:grid-cols-3">
                    <li className="py-5 sm:pr-5">
                      <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-gray-400">01</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-gray-200">
                        {inputMode === "none" ? "Start" : "Select input"}
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-gray-400">
                        {inputMode === "none"
                          ? "No source file is required for this workflow."
                          : "Use a PDF, LaTeX file, Word document, or project directory."}
                      </p>
                    </li>
                    <li className="border-t border-gray-200 py-5 sm:border-l sm:border-t-0 sm:px-5 dark:border-gray-800">
                      <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-gray-400">02</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-gray-200">
                        Run a workflow
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-gray-400">
                        Prompts execute in parallel or in sequence.
                      </p>
                    </li>
                    <li className="border-t border-gray-200 py-5 sm:border-l sm:border-t-0 sm:pl-5 dark:border-gray-800">
                      <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-gray-400">03</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-gray-200">
                        Review report(s)
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-gray-400">
                        Findings converge in a single review.
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
                                   focus-visible:ring-gray-400 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-900"
                      >
                        Start a new run
                      </button>
                    </div>
                  ) : (
                    <>
                      <p className="mt-6 text-center text-xs leading-relaxed text-gray-500 dark:text-gray-400">
                        This may take 15–60 minutes depending on<br />
                        paper length, number of agents, and LLM load.
                      </p>
                      <button
                        type="button"
                        onClick={cancel}
                        className="mx-auto mt-4 block rounded-lg px-3 py-1.5 text-xs font-medium text-gray-500 transition-colors
                                   hover:bg-red-50 hover:text-red-600 focus-visible:outline-none focus-visible:ring-2
                                   focus-visible:ring-red-300 dark:text-gray-400 dark:hover:bg-red-950/40 dark:hover:text-red-400"
                      >
                        Cancel run
                      </button>
                    </>
                  )}
                </div>
              </div>
            )}
            </Suspense>
          </div>

          {logs.length > 0 && <Console logs={logs} usage={usage} />}
        </main>
      </div>
    </div>
  );
}

export default App;
