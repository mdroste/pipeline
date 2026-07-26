import { lazy, Suspense, useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
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
import type { DepsReport, VarSpec, InputSlot } from "./lib/types";

const SettingsPage = lazy(() => import("./components/SettingsPage"));
const PipelinePage = lazy(() => import("./components/PipelinePage"));
const AboutPage = lazy(() => import("./components/AboutPage"));
const HistoryPage = lazy(() => import("./components/HistoryPage"));
const BatchPanel = lazy(() => import("./components/BatchPanel"));
const ReportWorkspace = lazy(() => import("./components/ReportWorkspace"));

function App() {
  const { state, logs, usage, startPipeline, rerunPipeline, cancel, reset, listenersReady, runStartedAt, passTimes } = usePipeline();
  const [paperPath, setPaperPath] = useState<string | null>(null);
  const [depsReport, setDepsReport] = useState<DepsReport | null>(null);
  const [depsLoading, setDepsLoading] = useState(true);
  const [page, setPage] = useState<AppPage>("main");
  const [configVersion, setConfigVersion] = useState(0);
  const [selectionKey, setSelectionKey] = useState(0);
  const [workflowDirty, setWorkflowDirty] = useState(false);
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
  // Variables and extra input slots the active profile declares; both drive
  // the pre-run options modal.
  const [profileVars, setProfileVars] = useState<VarSpec[]>([]);
  const [inputSlots, setInputSlots] = useState<InputSlot[]>([]);
  const [varModalOpen, setVarModalOpen] = useState(false);
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

  useEffect(() => {
    invoke<DepsReport>("check_deps")
      .then((report) => {
        setDepsReport(report);
        setDepsLoading(false);
      })
      .catch((e) => {
        console.error("Startup check failed:", e);
        setDepsReport({ deps: [], ready: false });
        setDepsLoading(false);
      });
  }, []);

  useEffect(() => {
    invoke<{ extraction?: { input_mode?: string; extra_inputs?: InputSlot[] }; variables?: VarSpec[] }>("get_pipeline_config")
      .then((c) => {
        setInputMode(c.extraction?.input_mode || "document");
        setProfileVars(c.variables ?? []);
        setInputSlots(c.extraction?.extra_inputs ?? []);
      })
      .catch(() => {
        setInputMode("document");
        setProfileVars([]);
        setInputSlots([]);
      });
  }, [configVersion]);

  const isRunning =
    state.kind !== "idle" && state.kind !== "done" && state.kind !== "error";
  const hasCurrentRun = state.kind !== "idle";
  const showRunSetup = page === "main" && state.kind === "idle";

  const launch = (variables?: Record<string, string>, extraInputs?: Record<string, string>) => {
    setPage("main");
    startPipeline(paperPath ?? "", false, variables, extraInputs);
  };

  const handleGenerate = () => {
    if (!(paperPath || inputMode === "none")) return;
    // Collect run-time variables and/or extra inputs first if the profile
    // declares any.
    if (profileVars.length > 0 || inputSlots.length > 0) {
      setVarModalOpen(true);
      return;
    }
    launch();
  };

  const handleNewRun = () => {
    if (isRunning) {
      setPage("main");
      return;
    }
    if (
      page === "pipeline" &&
      workflowDirty &&
      !window.confirm("You have unsaved workflow changes. Leave and discard them?")
    ) {
      return;
    }
    reset();
    setPaperPath(null);
    setHistoryRunId(null);
    setVarModalOpen(false);
    setSelectionKey((key) => key + 1);
    setPage("main");
  };

  const handleNavigate = (nextPage: AppPage) => {
    if (
      page === "pipeline" &&
      nextPage !== "pipeline" &&
      workflowDirty &&
      !window.confirm("You have unsaved workflow changes. Leave and discard them?")
    ) {
      return;
    }
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
        />
      )}

      {varModalOpen && (
        <VariablePrompt
          variables={profileVars}
          inputSlots={inputSlots}
          onCancel={() => setVarModalOpen(false)}
          onSubmit={(values, inputs) => {
            setVarModalOpen(false);
            launch(values, inputs);
          }}
        />
      )}

      {/* Invisible window-wide drag strip along the very top edge (macOS).
          Thin (16px) so it stays above the content panels' own controls,
          which start at 8px padding — grab the top edge anywhere to drag. */}
      {isMac && (
        <div data-tauri-drag-region className="fixed inset-x-0 top-0 z-30 h-4" />
      )}

      <div className="flex min-h-0 flex-1">
        <NavRail
          activePage={page}
          hasCurrentRun={hasCurrentRun}
          runInProgress={isRunning}
          isMac={isMac}
          dependenciesReady={depsReport?.ready ?? null}
          dependenciesLoading={depsLoading}
          width={navRailWidth}
          onResize={setNavRailWidth}
          onNewRun={handleNewRun}
          onNavigate={handleNavigate}
          onDependencies={() => depsReport && setShowDeps(true)}
        />

        {showRunSetup && (
          <RunSetupPanel
            configVersion={configVersion}
            dependenciesLoading={depsLoading}
            inputMode={inputMode}
            listenersReady={listenersReady}
            paperPath={paperPath}
            selectionKey={selectionKey}
            width={runSetupWidth}
            onConfigureWorkflow={() => setPage("pipeline")}
            onGenerate={handleGenerate}
            onPaperPathChange={setPaperPath}
            onProfileChange={() => setConfigVersion((version) => version + 1)}
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
                onProfileChange={() => setConfigVersion((v) => v + 1)}
                showBack={false}
              />
            ) : page === "help" ? (
              <AboutPage onClose={() => setPage("main")} showBack={false} />
            ) : page === "settings" ? (
              <SettingsPage onClose={() => setPage("main")} showBack={false} dark={dark} onDarkChange={handleDarkChange} />
            ) : page === "history" ? (
              <HistoryPage
                onClose={() => setPage("main")}
                showClose={false}
                initialRunId={historyRunId}
                onRerun={(runId, onlyFailed) => {
                  setPage("main");
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
                      <span className="text-xs font-medium tabular-nums text-gray-400 dark:text-gray-600">01</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-gray-200">
                        {inputMode === "none" ? "Start" : "Select input"}
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-gray-500">
                        {inputMode === "none"
                          ? "No source file is required for this workflow."
                          : "Use a PDF, LaTeX file, Word document, or project directory."}
                      </p>
                    </li>
                    <li className="border-t border-gray-200 py-5 sm:border-l sm:border-t-0 sm:px-5 dark:border-gray-800">
                      <span className="text-xs font-medium tabular-nums text-gray-400 dark:text-gray-600">02</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-gray-200">
                        Run a workflow
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-gray-500">
                        Prompts execute in parallel or in sequence.
                      </p>
                    </li>
                    <li className="border-t border-gray-200 py-5 sm:border-l sm:border-t-0 sm:pl-5 dark:border-gray-800">
                      <span className="text-xs font-medium tabular-nums text-gray-400 dark:text-gray-600">03</span>
                      <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-gray-200">
                        Review report(s)
                      </p>
                      <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-gray-500">
                        Findings converge in a single review.
                      </p>
                    </li>
                  </ol>
                </div>
              </div>
            ) : (
              <div className="flex h-full items-center justify-center px-8 py-12">
                <div className="w-full max-w-sm">
                  <PipelineProgress state={state} runStartedAt={runStartedAt} passTimes={passTimes} />
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
                      <p className="mt-6 text-center text-xs leading-relaxed text-gray-400 dark:text-gray-500">
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
