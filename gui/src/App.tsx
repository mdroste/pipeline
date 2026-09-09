import { useAppAppearance } from "./hooks/useAppAppearance";
import { useAppNotifications } from "./hooks/useAppNotifications";
import { useAppController } from "./hooks/useAppController";
import ProviderLimitBanner from "./components/ProviderLimitBanner";
import { lazy, Suspense } from "react";
import PipelineProgress from "./components/PipelineProgress";
import DepsCheck from "./components/DepsCheck";
import Console from "./components/Console";
import VariablePrompt from "./components/VariablePrompt";
import RunPreview from "./components/RunPreview";
import UpdateBanner from "./components/UpdateBanner";
import NavRail from "./components/NavRail";
import RunSetupPanel from "./components/RunSetupPanel";
import ErrorBoundary from "./components/ErrorBoundary";
import { isMac } from "./lib/platform";
import { notify } from "./components/DialogService";

const SettingsPage = lazy(() => import("./components/SettingsPage"));
const HomePage = lazy(() => import("./components/HomePage"));
const TasksPage = lazy(() => import("./components/TasksPage"));
const WorkspacePage = lazy(() => import("./components/WorkspacePage"));
const PipelinePage = lazy(() => import("./components/PipelinePage"));
const ResearchActivity = lazy(() => import("./components/ResearchActivity"));
const AboutPage = lazy(() => import("./components/AboutPage"));
const HistoryPage = lazy(() => import("./components/HistoryPage"));
const ProjectsPage = lazy(() => import("./components/ProjectsPage"));
const WorkflowGalleryPage = lazy(
  () => import("./components/WorkflowGalleryPage"),
);
const loadBatchPanel = () => import("./components/BatchPanel");
const BatchPanel = lazy(loadBatchPanel);
const ReportWorkspace = lazy(() => import("./components/ReportWorkspace"));

function App() {
  useAppAppearance();
  const {
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
  } = useAppController();
  useAppNotifications(state);

  return (
    <div
      data-testid="app-shell"
      className="flex h-screen flex-col overflow-hidden bg-gray-50 dark:bg-[#101010]"
    >
      {showDeps && depsReport && (
        <DepsCheck
          report={depsReport}
          onDismiss={() => setShowDeps(false)}
          onOpenPdfSettings={() =>
            void (async () => {
              if (!(await confirmLeaveCurrentPage("settings"))) return;
              setShowDeps(false);
              setSettingsInitialSection("extraction");
              setSettingsTargetId("paddleocr-local-engine");
              setSettingsNavigationKey((key) => key + 1);
              setPageState("settings");
            })()
          }
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
        <div
          data-tauri-drag-region
          className="fixed inset-x-0 top-0 z-30 h-4"
        />
      )}

      {closeProtectionUnavailable && (
        <div
          role="alert"
          className="flex shrink-0 items-center gap-3 border-b border-amber-300 bg-amber-50 px-5 py-2 text-xs text-amber-900
                     dark:border-amber-800 dark:bg-amber-950/60 dark:text-amber-200"
        >
          <span className="flex-1">
            Window-close protection is unavailable. Save workflow and settings
            changes before quitting.
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

      {researchNotice && (
        <div
          role="status"
          className="absolute bottom-5 right-5 z-50 max-w-sm space-y-2 rounded-lg border border-amber-300 bg-white p-4 shadow-lg dark:bg-neutral-950"
        >
          <p className="text-sm">A tracked research state changed.</p>
          <button
            className="rounded border px-3 py-1 text-xs"
            onClick={() => void inspectResearchAttention()}
          >
            Inspect research attention
          </button>
          <button
            className="px-3 py-1 text-xs"
            onClick={() => setResearchNotice(null)}
          >
            Dismiss notification
          </button>
        </div>
      )}
      {showActivity && (
        <Suspense fallback={null}>
          <ResearchActivity
            onClose={() => setShowActivity(false)}
            onSession={(id) => void openWorkspaceSession(id)}
            onProject={(id) => void openActivityProject(id)}
            onTasks={(id) => void openActivityTask(id)}
            onReview={(id) => void openActivityReview(id)}
          />
        </Suspense>
      )}
      <div className="flex min-h-0 flex-1">
        <NavRail
          onActivity={() => setShowActivity(true)}
          tasksAttention={tasksAttention}
          activePage={page}
          hasCurrentRun={hasCurrentRun}
          runInProgress={isRunning}
          workspaceActive={workspaceActive}
          workspaceAttention={workspaceAttention}
          isMac={isMac}
          dependenciesReady={depsReport?.ready ?? null}
          dependenciesLoading={depsLoading}
          dependenciesError={depsError}
          width={navRailWidth}
          onResize={setNavRailWidth}
          onNewRun={handleNewRun}
          onNavigate={(next) => {
            if (next === "workspace") openWorkspace("project");
            else void handleNavigate(next);
          }}
          recentProjects={recentProjects}
          recentProjectsLoading={projectsLoading}
          onOpenProject={openWorkspaceProject}
          onDependencies={() => {
            void checkDependencies().then((report) => {
              if (report) setShowDeps(true);
            });
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
            localLlmActive={
              depsReport?.deps.some(
                (dependency) =>
                  dependency.name === "Local LLM server" && dependency.found,
              ) ?? false
            }
            paperPath={paperPath}
            preparingRun={preparingRun}
            parallelOverrides={parallelOverrides}
            selectionKey={selectionKey}
            width={runSetupWidth}
            onConfigureWorkflow={() => void handleNavigate("pipeline")}
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
              void openHelp("privacy");
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
              onOpenSettings={() =>
                void (async () => {
                  if (!(await confirmLeaveCurrentPage("settings"))) return;
                  setSettingsInitialSection("workflow");
                  setSettingsTargetId("usage-limit-fallback");
                  setSettingsNavigationKey((key) => key + 1);
                  setPageState("settings");
                })()
              }
            />
          )}
          <div
            className={`min-h-0 flex-1 ${page === "settings" ? "overflow-hidden" : "overflow-auto"}`}
          >
            <Suspense
              fallback={
                <div className="flex items-center justify-center h-full text-gray-400">
                  <div className="animate-spin w-6 h-6 border-2 border-gray-300 border-t-gray-600 rounded-full" />
                </div>
              }
            >
              {page === "home" ? (
                <HomePage
                  projects={recentProjects}
                  projectsLoading={projectsLoading}
                  hasCurrentReview={hasCurrentRun || batchActive}
                  reviewRunning={isRunning}
                  onAssistant={() => void openWorkspace("chat")}
                  onNewProject={() => void createWorkspaceProject()}
                  onOpenProject={(id) => void openWorkspaceProject(id)}
                  onNewReview={() => void handleNewRun()}
                  onContinueReview={() =>
                    setPage(batchActive && !hasCurrentRun ? "batch" : "main")
                  }
                  onNavigate={(next) => void handleNavigate(next)}
                />
              ) : page === "workspace" ? (
                <WorkspacePage
                  entryRequest={workspaceEntry.request}
                  entrySurface={workspaceEntry.surface}
                  newProjectRequest={newProjectRequest}
                  onNewProjectRequestHandled={() => setNewProjectRequest(0)}
                  onTasks={(session, id) => void openTask(session, id)}
                  onOpenSettings={() => void openWorkspaceSettings()}
                  onReviewHandoff={(handoff) =>
                    void handleWorkspaceReviewHandoff(handoff)
                  }
                />
              ) : page === "tasks" ? (
                <TasksPage
                  initialSessionId={taskSessionId}
                  initialTaskId={taskId}
                  onConversation={async (id) => {
                    await openWorkspaceSession(id);
                  }}
                />
              ) : page === "pipeline" ? (
                <PipelinePage
                  onClose={() => void closePipeline()}
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
                  onDirtyChange={setProjectsDirty}
                  onSaveHandlerChange={registerProjectSave}
                  onOpenRun={(runId, source) =>
                    void openHistoryRun(runId, source ?? null)
                  }
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
                    durationSecs={
                      runStartedAt ? (Date.now() - runStartedAt) / 1000 : null
                    }
                  />
                </ErrorBoundary>
              ) : state.kind === "idle" ? (
                <div className="flex items-center justify-center min-h-full px-8 py-16">
                  <div className="w-full max-w-3xl">
                    <ol className="grid grid-cols-1 border-y border-gray-200 dark:border-neutral-800 sm:grid-cols-3">
                      <li className="py-5 sm:pr-5">
                        <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-neutral-400">
                          01
                        </span>
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
                        <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-neutral-400">
                          02
                        </span>
                        <p className="mt-2 text-sm font-semibold text-gray-800 dark:text-neutral-200">
                          Select a workflow
                        </p>
                        <p className="mt-1 text-sm leading-5 text-gray-500 dark:text-neutral-400">
                          Choose from built-in workflows or customize your own.
                        </p>
                      </li>
                      <li className="border-t border-gray-200 py-5 sm:border-l sm:border-t-0 sm:pl-5 dark:border-neutral-800">
                        <span className="text-xs font-medium tabular-nums text-gray-500 dark:text-neutral-400">
                          03
                        </span>
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
                          This may take 15–60 minutes depending on
                          <br />
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

          {logs.length > 0 && (
            <Console logs={logs} usage={usage} active={isRunning} />
          )}
        </main>
      </div>
    </div>
  );
}

export default App;
