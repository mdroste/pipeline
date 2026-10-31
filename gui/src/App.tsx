import { useAppAppearance } from "./hooks/useAppAppearance";
import { useAppNotifications } from "./hooks/useAppNotifications";
import { useAppController } from "./hooks/useAppController";
import ProviderLimitBanner from "./components/ProviderLimitBanner";
import { lazy, Suspense, useLayoutEffect, useMemo, useRef } from "react";
import ReviewRunView from "./components/ReviewRunView";
import DepsCheck from "./components/DepsCheck";
import Console from "./components/Console";
import VariablePrompt from "./components/VariablePrompt";
import RunPreview from "./components/RunPreview";
import UpdateBanner from "./components/UpdateBanner";
import NavRail, { CollapsedNavBar } from "./components/NavRail";
import RunSetupPanel from "./components/RunSetupPanel";
import RunPlanPreview from "./components/RunPlanPreview";
import ErrorBoundary from "./components/ErrorBoundary";
import GetStartedHost from "./components/get-started/GetStartedHost";
import { openGetStarted } from "./lib/getStarted";
import { HistoryRunOpener } from "./hooks/app/historyRunOpener";
import WindowControls from "./components/WindowControls";
import { isMac, isWindows } from "./lib/platform";
import { router, type AppPage } from "./lib/router";
import { confirmUpdateRestart } from "./lib/updateRestart";
import { confirmDialog, notify } from "./components/DialogService";
import type { QueuedRun } from "./lib/types";

const SettingsPage = lazy(() => import("./components/SettingsPage"));
const HomePage = lazy(() => import("./components/HomePage"));
const TasksPage = lazy(() => import("./components/TasksPage"));
const ProjectIndexPage = lazy(() => import("./components/ProjectIndexPage"));
const WorkspacePage = lazy(() => import("./components/WorkspacePage"));
const PipelinePage = lazy(() => import("./components/PipelinePage"));
const ResearchActivity = lazy(() => import("./components/ResearchActivity"));
const AboutPage = lazy(() => import("./components/AboutPage"));
const HistoryPage = lazy(() => import("./components/HistoryPage"));
const ProjectsPage = lazy(() => import("./components/ProjectsPage"));
const WorkflowGalleryPage = lazy(
  () => import("./components/WorkflowGalleryPage"),
);
// Pages that start on the plain surface rather than the shell behind it. The
// Windows title strip takes the same color so it reads as part of the page.
const SURFACE_TOP_PAGES: ReadonlySet<AppPage> = new Set([
  "batch",
  "gallery",
  "history",
  "home",
  "project-index",
  "projects",
  "workspace",
]);

const loadBatchPanel = () => import("./components/BatchPanel");
const BatchPanel = lazy(loadBatchPanel);
const ReportWorkspace = lazy(() => import("./components/ReportWorkspace"));

function App() {
  useAppAppearance();
  const {
    navigation,
    setup,
    reviewLaunch,
    attention,
    appearance,
    projects,
    reviewView,
    closePipeline,
    openActivityReview,
  } = useAppController();
  const {
    openWorkspaceSession,
    openActivityProject,
    openActivityTask,
    page,
    handleNavigate,
    openWorkspaceProject,
    openWorkspace,
    openHelp,
    createWorkspaceProject,
    setPage,
    workspaceEntry,
    newProjectRequest,
    setNewProjectRequest,
    openTask,
    openWorkspaceSettings,
    taskSessionId,
    taskId,
    discoveryId,
    automatePath,
    taskEntryRequest,
    setTasksDirty,
    setWorkflowDirty,
    openPaddleInstallSettings,
    helpInitialSection,
    setSettingsDirty,
    settingsInitialSection,
    settingsTargetId,
    settingsNavigationKey,
    historyRunId,
    route,
    historySourceSelection,
    openHistoryRun,
    registerProjectSave,
    registerWorkspaceSave,
    setProjectsDirty,
  } = navigation;
  const {
    showDeps,
    depsReport,
    setShowDeps,
    pendingRun,
    setPendingRun,
    depsLoading,
    depsError,
    checkDependencies,
    configVersion,
    runConfigError,
    setRunConfigError,
    runConfigLoading,
    inputMode,
    inputSelection,
    paperPath,
    parallelOverrides,
    selectionKey,
    setPaperPath,
    setInputSelection,
    setParallelOverrides,
    handleProfileChange,
    loadRunSetup,
    setConfigVersion,
  } = setup;
  const {
    launch,
    runPreview,
    setRunPreview,
    launchError,
    launchRetry,
    clearLaunchError,
    startPreparedLaunch,
    handleNewRun,
    preparingRun,
    handleGenerate,
    handleWorkspaceReviewHandoff,
    openRun,
    rerunReview,
  } = reviewLaunch;
  const {
    researchNotice,
    inspectResearchAttention,
    setResearchNotice,
    tasksAttention,
    workspaceActive,
    workspaceAttention,
  } = attention;
  const {
    closeProtectionUnavailable,
    setCloseProtectionUnavailable,
    navRailWidth,
    setNavRailWidth,
    navRailCollapsed,
    toggleNavRail,
    navRailToggleRef,
    runSetupWidth,
    setRunSetupWidth,
    theme,
    handleThemeChange,
  } = appearance;
  const { recentProjects, projectsLoading } = projects;
  const {
    hasCurrentRun,
    isRunning,
    showRunSetup,
    listenersReady,
    providerLimitNotices,
    batchActive,
    state,
    runQueue,
    selectedRun,
    seenRuns,
    runningReviews,
    continueRunKey,
    dismissRuns,
    cancelRun,
    activeRunPlan,
    stageHistory,
    runStartedAt,
    passTimes,
    reviewRouting,
    logs,
    usage,
  } = reviewView;
  useAppNotifications(runQueue.runs);
  // Every destination shares the content scroller below, so open each new
  // page, review, or saved run at the top. A layout effect runs before pages'
  // passive effects, so a page that restores its own offset still wins.
  const contentRef = useRef<HTMLDivElement>(null);
  const routeRunKey = route.page === "main" ? (route.runKey ?? null) : null;
  useLayoutEffect(() => {
    if (contentRef.current) contentRef.current.scrollTop = 0;
  }, [page, routeRunKey, historyRunId]);
  // Finished reviews not opened since they finished get a marker in the rail.
  const unseenRuns = useMemo(
    () =>
      new Set(
        runQueue.runs
          .filter(
            (run) =>
              (run.status === "done" || run.status === "failed") &&
              !seenRuns.has(run.key),
          )
          .map((run) => run.key),
      ),
    [runQueue.runs, seenRuns],
  );
  const runActivity = {
    running: runningReviews,
    queued: runQueue.runs.filter((run) => run.status === "queued").length,
    limit: runQueue.limit,
  };
  const runSlotsFull =
    runActivity.running + runActivity.queued >= runActivity.limit;
  const cancelReview = async (run: QueuedRun) => {
    if (
      run.status === "running" &&
      !(await confirmDialog(
        `Cancel the review of ${run.label}? Completed steps stay in Review history and can be resumed.`,
        {
          title: "Cancel review?",
          confirmLabel: "Cancel review",
          cancelLabel: "Keep running",
          destructive: true,
        },
      ))
    )
      return;
    await cancelRun(run.key).catch((error: unknown) =>
      notify(
        `The review could not be cancelled: ${error instanceof Error ? error.message : String(error)}`,
      ),
    );
  };
  const dismissReviews = (key?: string) => {
    void dismissRuns(key).catch((error: unknown) =>
      notify(
        `The list could not be updated: ${error instanceof Error ? error.message : String(error)}`,
      ),
    );
  };

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
            void router
              .navigate({
                page: "settings",
                section: "extraction",
                targetId: "paddleocr-local-engine",
              })
              .then((allowed) => {
                if (allowed) setShowDeps(false);
              })
          }
        />
      )}

      <GetStartedHost
        dependencies={depsReport}
        dependenciesLoading={depsLoading}
        onCheckDependencies={() => void checkDependencies()}
        onNewReview={() => void handleNewRun()}
        onNewProject={() => void createWorkspaceProject()}
      />

      {pendingRun && (
        <VariablePrompt
          variables={pendingRun.variables}
          inputSlots={pendingRun.inputSlots}
          initialValues={launchRetry?.values}
          initialInputs={launchRetry?.inputs}
          error={launchRetry?.message}
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
          queued={runSlotsFull}
          onCancel={() => setRunPreview(null)}
          onRun={() => void startPreparedLaunch()}
        />
      )}

      {/* Invisible window-wide drag strip along the very top edge (macOS).
          Only 6px tall, so it clears the controls in top bars and banners;
          those bars' backgrounds drag the window as well ("deep" regions,
          where buttons and other controls still take their own clicks). */}
      {isMac && (
        <div
          data-tauri-drag-region
          className="fixed inset-x-0 top-0 z-30 h-1.5"
        />
      )}
      {isWindows && <WindowControls />}

      {closeProtectionUnavailable && (
        <div
          role="alert"
          data-tauri-drag-region={isMac || isWindows ? "deep" : undefined}
          className={`flex shrink-0 items-center gap-3 border-b border-amber-300 bg-amber-50 py-2 pl-5 text-xs text-amber-900
                     dark:border-amber-800 dark:bg-amber-950/60 dark:text-amber-200 ${isWindows ? "pr-[150px]" : "pr-5"}`}
        >
          <span className="flex-1">
            Pipeline cannot warn you about unsaved changes when you quit. Save
            your review designs and settings first.
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
          <p className="text-sm">
            Something you’re tracking in this project has changed.
          </p>
          <button
            className="rounded border px-3 py-1 text-xs"
            onClick={() => void inspectResearchAttention()}
          >
            View changes
          </button>
          <button
            className="px-3 py-1 text-xs"
            onClick={() => setResearchNotice(null)}
          >
            Dismiss notification
          </button>
        </div>
      )}
      {navRailCollapsed && (
        <CollapsedNavBar
          isMac={isMac}
          isWindows={isWindows}
          status={
            workspaceAttention || tasksAttention || unseenRuns.size > 0
              ? "attention"
              : workspaceActive || isRunning
                ? "running"
                : null
          }
          onExpand={toggleNavRail}
          toggleRef={navRailToggleRef}
        />
      )}
      <div className="flex min-h-0 flex-1">
        <NavRail
          collapsed={navRailCollapsed}
          onToggleCollapsed={toggleNavRail}
          toggleRef={navRailToggleRef}
          onActivity={() => void handleNavigate("activity")}
          tasksAttention={tasksAttention}
          activePage={page}
          hasCurrentRun={hasCurrentRun}
          runs={runQueue.runs}
          runLimit={runQueue.limit}
          activeRunKey={selectedRun?.key ?? null}
          unseenRuns={unseenRuns}
          onOpenRun={(key) => void openRun(key)}
          onCancelRun={(run) => void cancelReview(run)}
          onDismissRuns={dismissReviews}
          onConfigureRuns={() =>
            void router.navigate({
              page: "settings",
              section: "workflow",
              targetId: "max-concurrent-reviews",
            })
          }
          workspaceActive={workspaceActive}
          workspaceAttention={workspaceAttention}
          isMac={isMac}
          isWindows={isWindows}
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
            inputSelection={inputSelection}
            launchError={launchError}
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
            runActivity={runActivity}
            onConfigureWorkflow={() => void handleNavigate("pipeline")}
            onGenerate={handleGenerate}
            onPaperPathChange={(path) => {
              setPaperPath(path);
              if (path) setRunConfigError(null);
            }}
            onInputSelectionChange={(selection) => {
              setInputSelection(selection);
              setPaperPath(selection?.paths[0] ?? null);
              clearLaunchError();
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
          {/* The Windows title bar: the window controls float over its right
              end. The collapsed sidebar's bar does this job when it is shown. */}
          {isWindows && !navRailCollapsed && (
            <div
              data-testid="window-drag-strip"
              data-tauri-drag-region
              className={`h-8 shrink-0 ${
                SURFACE_TOP_PAGES.has(page) ||
                (page === "main" && state.kind === "done")
                  ? "bg-surface"
                  : ""
              }`}
            />
          )}
          <UpdateBanner
            confirmRestart={() => {
              const unsaved = navigation.unsavedRef.current;
              return confirmUpdateRestart({
                unsavedChanges:
                  unsaved.workflowDirty ||
                  unsaved.settingsDirty ||
                  unsaved.projectsDirty ||
                  unsaved.tasksDirty,
                reviews: isRunning,
                conversations: workspaceActive,
              });
            }}
          />
          {page === "main" && providerLimitNotices.length > 0 && (
            <ProviderLimitBanner
              notices={providerLimitNotices}
              onOpenSettings={() =>
                void router.navigate({
                  page: "settings",
                  section: "workflow",
                  targetId: "usage-limit-fallback",
                })
              }
            />
          )}
          <div
            ref={contentRef}
            data-testid="app-content"
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
                  hasCurrentReview={continueRunKey !== null || batchActive}
                  reviewRunning={isRunning}
                  runningReviews={runningReviews}
                  onAssistant={() => void openWorkspace("chat")}
                  onNewProject={() => void createWorkspaceProject()}
                  onOpenProject={(id) => void openWorkspaceProject(id)}
                  onNewReview={() => void handleNewRun()}
                  onContinueReview={() =>
                    continueRunKey
                      ? void openRun(continueRunKey)
                      : setPage(batchActive ? "batch" : "main")
                  }
                  onNavigate={(next) => void handleNavigate(next)}
                  onSetUp={() => openGetStarted("setup")}
                  onExampleReview={() => openGetStarted("example")}
                />
              ) : page === "project-index" ? (
                <ProjectIndexPage
                  onOpenProject={openWorkspaceProject}
                  onResumeConversation={openWorkspaceSession}
                />
              ) : page === "workspace" ? (
                // A project's Reviews open saved runs (and cited sources)
                // through the shell, like the other review entry points.
                <HistoryRunOpener.Provider value={openHistoryRun}>
                  <WorkspacePage
                    onAllProjects={() => void handleNavigate("project-index")}
                    onSaveHandlerChange={registerWorkspaceSave}
                    entryTarget={workspaceEntry.target}
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
                </HistoryRunOpener.Provider>
              ) : page === "tasks" ? (
                <TasksPage
                  key={
                    discoveryId ? `${discoveryId}:${taskEntryRequest}` : "tasks"
                  }
                  entryRequest={taskEntryRequest}
                  initialDiscoveryId={discoveryId}
                  initialSessionId={taskSessionId}
                  initialTaskId={taskId}
                  onDirtyChange={setTasksDirty}
                  initialAutomatePath={automatePath}
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
                  onRerun={(runId, onlyFailed) =>
                    void rerunReview(runId, onlyFailed)
                  }
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
              ) : page === "activity" ? (
                <ResearchActivity
                  asPage
                  onClose={() => setPage("home")}
                  onSession={(id) => void openWorkspaceSession(id)}
                  onProject={(id) => void openActivityProject(id)}
                  onTasks={(id) => void openActivityTask(id)}
                  onReview={(id) => void openActivityReview(id)}
                />
              ) : page === "batch" ? (
                <BatchPanel
                  onClose={() => setPage("main")}
                  showClose={false}
                  onOpenRun={(runId) => void openHistoryRun(runId, null)}
                />
              ) : state.kind === "done" ? (
                // A rendering defect in the finished-run view must not take
                // down the whole app shell with it.
                <ErrorBoundary nested>
                  <ReportWorkspace
                    runId={state.runId}
                    markdown={state.markdown}
                    report={state.report}
                    extractedText={state.extractedText}
                    durationSecs={
                      selectedRun?.startedAt && selectedRun.finishedAt
                        ? (selectedRun.finishedAt - selectedRun.startedAt) /
                          1000
                        : null
                    }
                  />
                </ErrorBoundary>
              ) : hasCurrentRun ? (
                <ReviewRunView
                  run={selectedRun}
                  runs={runQueue.runs}
                  limit={runQueue.limit}
                  state={state}
                  plan={activeRunPlan}
                  stageHistory={stageHistory}
                  runStartedAt={runStartedAt}
                  passTimes={passTimes}
                  reviewRouting={reviewRouting}
                  onCancel={() => selectedRun && void cancelReview(selectedRun)}
                  onNewReview={() => void handleNewRun()}
                  onOpenHistory={(runId) =>
                    void openHistoryRun(runId ?? "", null)
                  }
                />
              ) : (
                <RunPlanPreview
                  refreshKey={configVersion}
                  inputMode={inputMode}
                  hasInput={!!paperPath}
                  onOpenDesigner={() => void handleNavigate("pipeline")}
                />
              )}
            </Suspense>
          </div>

          {/* The log dock belongs to the run view, not to every destination. */}
          {page === "main" && logs.length > 0 && (
            <Console
              logs={logs}
              usage={usage}
              active={selectedRun?.status === "running"}
            />
          )}
        </main>
      </div>
    </div>
  );
}

export default App;
