import { lazy, Suspense, useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import PaperSelector from "./components/PaperSelector";
import PipelineProgress from "./components/PipelineProgress";
import WorkflowPanel from "./components/WorkflowPanel";
import ExportControls from "./components/ExportControls";
import DepsCheck from "./components/DepsCheck";
import Console from "./components/Console";
import VariablePrompt from "./components/VariablePrompt";
import UpdateBanner from "./components/UpdateBanner";
import ResizeHandle from "./components/ResizeHandle";
import { usePipeline } from "./hooks/usePipeline";
import { isMac } from "./lib/platform";
import { detectReportIssues } from "./lib/issues";
import { renderSurvey } from "./lib/surveyMarkdown";
import type { PipelineReport, DepsReport, VarSpec, InputSlot } from "./lib/types";

const SettingsPage = lazy(() => import("./components/SettingsPage"));
const PipelinePage = lazy(() => import("./components/PipelinePage"));
const AboutPage = lazy(() => import("./components/AboutPage"));
const HistoryPage = lazy(() => import("./components/HistoryPage"));
const BatchPanel = lazy(() => import("./components/BatchPanel"));
const ArtifactExplorer = lazy(() => import("./components/ArtifactExplorer"));
const ReportViewer = lazy(() => import("./components/ReportViewer"));
const IssuesTable = lazy(() => import("./components/IssuesTable"));

type Page = "main" | "pipeline" | "settings" | "help" | "history" | "batch";

function getArtifactMarkdown(
  artifact: string,
  reportMarkdown: string,
  extractedText: string,
  report: PipelineReport,
): string {
  if (artifact === "report") return reportMarkdown;
  if (artifact === "extracted_text") return extractedText;
  if (artifact === "orientation") return renderSurvey(report.orientation);
  if (artifact.startsWith("step:")) {
    const stepId = artifact.slice(5);
    const output = report.step_outputs.find((s) => s.step_id === stepId);
    if (output) {
      return `# ${output.step_label}\n\n**Phase**: ${output.phase} · **Agent**: ${output.agent || "default"}\n\n---\n\n${output.raw_text}`;
    }
  }
  return reportMarkdown;
}

function App() {
  const { state, logs, usage, startPipeline, rerunPipeline, cancel, reset, listenersReady, runStartedAt, passTimes } = usePipeline();
  const [paperPath, setPaperPath] = useState<string | null>(null);
  const [depsReport, setDepsReport] = useState<DepsReport | null>(null);
  const [depsLoading, setDepsLoading] = useState(true);
  const [page, setPage] = useState<Page>("main");
  const [configVersion, setConfigVersion] = useState(0);
  const [sidebarWidth, setSidebarWidth] = useState(320);
  const [artifact, setArtifact] = useState<string>("report");
  // Report vs. structured-issues view (shown only when issues are detected).
  const [reportView, setReportView] = useState<"report" | "issues">("report");
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

  const launch = (variables?: Record<string, string>, extraInputs?: Record<string, string>) => {
    setPage("main");
    setArtifact("report");
    startPipeline(paperPath ?? "", true, variables, extraInputs);
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

  const isRunning =
    state.kind !== "idle" && state.kind !== "done" && state.kind !== "error";

  // Structured issues detected in a finished report (enables the Issues view).
  const doneIssues = state.kind === "done" ? detectReportIssues(state.report) : null;

  const [showDeps, setShowDeps] = useState(false);

  // Auto-show deps modal on startup if required deps are missing
  useEffect(() => {
    if (depsReport && !depsReport.ready) {
      setShowDeps(true);
    }
  }, [depsReport]);

  return (
    <div data-testid="app-shell" className="h-screen bg-gray-50 dark:bg-gray-950 flex flex-col overflow-hidden">
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
        <div data-tauri-drag-region className="fixed top-0 inset-x-0 h-4 z-30" />
      )}

      <main className="flex-1 flex min-h-0">
        {/* Sidebar — on macOS it extends to the window top and hosts the
            traffic lights; the padding above the content is the drag region */}
        <aside
          className={`border-r border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 p-6 flex flex-col gap-4 overflow-y-auto shrink-0 relative ${isMac ? "pt-12" : ""}`}
          style={{ width: sidebarWidth }}
        >
          {isMac && (
            <div
              data-tauri-drag-region
              className="absolute top-0 left-0 right-0 h-12 z-10"
            />
          )}
          <PaperSelector
            onPathChange={(p) => {
              setPaperPath(p);
              if (state.kind === "error" || state.kind === "done") reset();
            }}
            disabled={isRunning}
          />
          {inputMode === "none" && (
            <p className="text-xs text-gray-400 dark:text-gray-500 -mt-2">
              This workflow needs no input — you can generate directly.
            </p>
          )}

          <WorkflowPanel
            disabled={isRunning}
            editorOpen={page === "pipeline"}
            onConfigure={() => setPage("pipeline")}
            onProfileChange={() => setConfigVersion((v) => v + 1)}
            refreshKey={configVersion}
          />

          <button
            onClick={handleGenerate}
            disabled={(!paperPath && inputMode !== "none") || isRunning || depsLoading || !listenersReady}
            className="w-full py-2.5 px-4 bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 rounded-lg font-medium
                       hover:bg-gray-800 dark:hover:bg-gray-200 disabled:bg-gray-300 dark:disabled:bg-gray-700 disabled:cursor-not-allowed
                       transition-colors"
          >
            {depsLoading
              ? "Checking dependencies..."
              : isRunning
                ? "Running..."
                : "Run"}
          </button>

          {isRunning && (
            <button
              onClick={cancel}
              className="w-full py-2 px-4 border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 rounded-lg
                         text-sm hover:bg-red-50 dark:hover:bg-red-950 transition-colors"
            >
              Cancel
            </button>
          )}

          {state.kind === "error" && (
            <div className="p-3 bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 rounded-lg text-sm text-red-700 dark:text-red-400">
              {state.message}
            </div>
          )}

          {/* Footer: app navigation + dependency status */}
          <div className="mt-auto pt-3 border-t border-gray-100 dark:border-gray-800 flex items-center gap-1 -mx-2 -mb-2">
            <button
              onClick={() => { setHistoryRunId(null); setPage(page === "history" ? "main" : "history"); }}
              className={`p-2 rounded-lg transition-colors ${
                page === "history"
                  ? "bg-gray-100 text-gray-900 dark:bg-gray-800 dark:text-gray-100"
                  : "text-gray-400 hover:text-gray-600 hover:bg-gray-50 dark:text-gray-500 dark:hover:text-gray-300 dark:hover:bg-gray-800"
              }`}
              title="Run history"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round"
                  d="M12 8v4l3 3m6-3a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" />
              </svg>
            </button>
            <button
              onClick={() => setPage(page === "batch" ? "main" : "batch")}
              className={`p-2 rounded-lg transition-colors ${
                page === "batch"
                  ? "bg-gray-100 text-gray-900 dark:bg-gray-800 dark:text-gray-100"
                  : "text-gray-400 hover:text-gray-600 hover:bg-gray-50 dark:text-gray-500 dark:hover:text-gray-300 dark:hover:bg-gray-800"
              }`}
              title="Batch run"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round"
                  d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5" />
              </svg>
            </button>
            <button
              onClick={() => setPage(page === "help" ? "main" : "help")}
              className={`p-2 rounded-lg transition-colors ${
                page === "help"
                  ? "bg-gray-100 text-gray-900 dark:bg-gray-800 dark:text-gray-100"
                  : "text-gray-400 hover:text-gray-600 hover:bg-gray-50 dark:text-gray-500 dark:hover:text-gray-300 dark:hover:bg-gray-800"
              }`}
              title="Help"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                <path strokeLinecap="round" strokeLinejoin="round"
                  d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9 5.25h.008v.008H12v-.008Z" />
              </svg>
            </button>
            <button
              onClick={() => setPage(page === "settings" ? "main" : "settings")}
              aria-label="Settings"
              className={`p-2 rounded-lg transition-colors ${
                page === "settings"
                  ? "bg-gray-100 text-gray-900 dark:bg-gray-800 dark:text-gray-100"
                  : "text-gray-400 hover:text-gray-600 hover:bg-gray-50 dark:text-gray-500 dark:hover:text-gray-300 dark:hover:bg-gray-800"
              }`}
              title="Settings"
            >
              <svg
                className="w-5 h-5"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={1.5}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.325.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 0 1 1.37.49l1.296 2.247a1.125 1.125 0 0 1-.26 1.431l-1.003.827c-.293.241-.438.613-.43.992a7.723 7.723 0 0 1 0 .255c-.008.378.137.75.43.991l1.004.827c.424.35.534.955.26 1.43l-1.298 2.247a1.125 1.125 0 0 1-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.47 6.47 0 0 1-.22.128c-.331.183-.581.495-.644.869l-.213 1.281c-.09.543-.56.94-1.11.94h-2.594c-.55 0-1.019-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 0 1-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 0 1-1.369-.49l-1.297-2.247a1.125 1.125 0 0 1 .26-1.431l1.004-.827c.292-.24.437-.613.43-.991a6.932 6.932 0 0 1 0-.255c.007-.38-.138-.751-.43-.992l-1.004-.827a1.125 1.125 0 0 1-.26-1.43l1.297-2.247a1.125 1.125 0 0 1 1.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.086.22-.128.332-.183.582-.495.644-.869l.214-1.28Z"
                />
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"
                />
              </svg>
            </button>
            {depsReport && !depsLoading && (
              <button
                onClick={() => setShowDeps(true)}
                className="ml-auto px-2 text-xs text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors text-right"
              >
                {depsReport.ready
                  ? "All dependencies OK"
                  : "Dependencies need attention"}
              </button>
            )}
          </div>
          <ResizeHandle onResize={setSidebarWidth} min={240} max={480} />
        </aside>

        {/* Main content */}
        <div className="flex-1 flex flex-col min-h-0">
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
              <PipelinePage onClose={() => { setPage("main"); setConfigVersion((v) => v + 1); }} onProfileChange={() => setConfigVersion((v) => v + 1)} />
            ) : page === "help" ? (
              <AboutPage onClose={() => setPage("main")} />
            ) : page === "settings" ? (
              <SettingsPage onClose={() => setPage("main")} dark={dark} onDarkChange={handleDarkChange} />
            ) : page === "history" ? (
              <HistoryPage
                onClose={() => setPage("main")}
                initialRunId={historyRunId}
                onRerun={(runId, onlyFailed) => {
                  setPage("main");
                  setArtifact("report");
                  rerunPipeline(runId, { onlyFailed });
                }}
              />
            ) : page === "batch" ? (
              <BatchPanel
                onClose={() => setPage("main")}
                onOpenRun={(runId) => { setHistoryRunId(runId); setPage("history"); }}
              />
            ) : state.kind === "done" ? (
              <div className="flex flex-col h-full">
                {/* Warning banner for failed steps */}
                {state.report.failed_steps && state.report.failed_steps.length > 0 && (
                  <div className="px-6 py-2.5 bg-amber-50 dark:bg-amber-950 border-b border-amber-200 dark:border-amber-800 text-sm text-amber-800 dark:text-amber-300">
                    <span className="font-medium">Incomplete report.</span>{" "}
                    {state.report.failed_steps.map((f) => f.step_label).join(", ")} failed and {state.report.failed_steps.length === 1 ? "is" : "are"} not reflected below.
                  </div>
                )}
                {/* Artifact selector */}
                <div className="flex items-center gap-3 px-6 py-2 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
                  <label className="text-xs text-gray-500 dark:text-gray-400 shrink-0">
                    {state.runId ? "Run artifacts" : "Viewing:"}
                  </label>
                  {doneIssues && (
                    <div className="flex items-center rounded-md border border-gray-300 dark:border-gray-600 overflow-hidden shrink-0">
                      <button
                        onClick={() => setReportView("report")}
                        className={`px-2 py-1 text-xs transition-colors ${reportView === "report" ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900" : "text-gray-600 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"}`}
                      >
                        Report
                      </button>
                      <button
                        onClick={() => setReportView("issues")}
                        className={`px-2 py-1 text-xs transition-colors ${reportView === "issues" ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900" : "text-gray-600 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"}`}
                      >
                        Issues ({doneIssues.length})
                      </button>
                    </div>
                  )}
                  {!state.runId && <select
                    value={artifact}
                    onChange={(e) => setArtifact(e.target.value)}
                    className="py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-sm text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                               focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
                  >
                    <option value="report">Final Report</option>
                    <option value="extracted_text">Extracted Text</option>
                    <option value="orientation">Orientation Map</option>
                    {state.report.step_outputs.map((s) => (
                      <option key={s.step_id} value={`step:${s.step_id}`}>
                        Step: {s.step_label}
                      </option>
                    ))}
                  </select>}
                  <div className="ml-auto">
                    <ExportControls
                      markdown={state.markdown}
                      report={state.report}
                      extractedText={state.extractedText}
                    />
                  </div>
                </div>
                <div className="flex-1 overflow-auto min-h-0">
                  {doneIssues && reportView === "issues" ? (
                    <IssuesTable issues={doneIssues} runId={state.runId} />
                  ) : state.runId ? (
                    <ArtifactExplorer runId={state.runId} fallbackMarkdown={state.markdown} />
                  ) : (
                    <ReportViewer markdown={getArtifactMarkdown(artifact, state.markdown, state.extractedText, state.report)} />
                  )}
                </div>
              </div>
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
                          : "Use a PDF, LaTeX file, or project directory."}
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

                  <div className="mt-6 flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400">
                    <span className="h-1.5 w-1.5 rounded-full bg-gray-900 dark:bg-gray-100" aria-hidden="true" />
                    {inputMode === "none"
                      ? "Use Run in the sidebar when you are ready."
                      : "Choose an input in the sidebar to begin."}
                  </div>
                </div>
              </div>
            ) : (
              <div className="flex items-center justify-center h-full">
                <div className="w-full max-w-sm">
                  <PipelineProgress state={state} runStartedAt={runStartedAt} passTimes={passTimes} />
                  <p className="text-xs text-gray-400 dark:text-gray-500 mt-6 text-center leading-relaxed">
                    This may take 15–60 minutes depending on<br />
                    paper length, number of agents, and LLM load.
                  </p>
                </div>
              </div>
            )}
            </Suspense>
          </div>

          {logs.length > 0 && <Console logs={logs} usage={usage} />}
        </div>
      </main>
    </div>
  );
}

export default App;
