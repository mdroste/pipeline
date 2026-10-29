import { useEffect, useState } from "react";
import { NAV_RAIL_WIDTH } from "../components/NavRail";
import { appClient } from "../lib/appClient";
import { EMPTY_USAGE } from "../lib/runViews";
import { router } from "../lib/router";
import type { PipelineState } from "../lib/reviewRuntimeTypes";
import { usePipeline } from "./usePipeline";
import usePersistentPanelWidth from "./usePersistentPanelWidth";
import useRecentProjects from "./useRecentProjects";
import { useAppAttention } from "./app/useAppAttention";
import {
  useAppNavigation,
  useAppRouteCommits,
  useRememberAppPage,
} from "./app/useAppNavigation";
import { useNativeCloseProtection } from "./app/useNativeCloseProtection";
import { useAppTheme } from "./app/useAppTheme";
import { useBatchActivity } from "./app/useBatchActivity";
import {
  useReviewSetup,
  useReviewSetupLifecycle,
  useReviewDependencyWarning,
} from "./app/useReviewSetup";
import { useReviewLaunch } from "./app/useReviewLaunch";

const IDLE: PipelineState = { kind: "idle" };

export function useAppController() {
  const {
    queue: runQueue,
    views: runViews,
    listenersReady,
    setupError: runEventsError,
    enqueue,
    enqueueBatch,
    enqueueRerun,
    cancel: cancelRun,
    dismiss: dismissRuns,
  } = usePipeline();
  const setup = useReviewSetup();
  const navigation = useAppNavigation();
  const { route, page } = navigation;
  const attention = useAppAttention(page);
  // The review on screen: `main` with a run key is one queued, running, or
  // finished review; without one it is the new-review setup.
  const selectedRunKey = route.page === "main" ? (route.runKey ?? null) : null;
  const selectedRun =
    runQueue.runs.find((run) => run.key === selectedRunKey) ?? null;
  const selectedView = selectedRunKey ? runViews[selectedRunKey] : undefined;
  const state = selectedView?.state ?? IDLE;
  // Finished reviews the user has not opened since they finished.
  const [seenRuns, setSeenRuns] = useState<ReadonlySet<string>>(new Set());
  useEffect(() => {
    if (!selectedRun || seenRuns.has(selectedRun.key)) return;
    if (["done", "failed", "cancelled"].includes(selectedRun.status))
      setSeenRuns((seen) => new Set(seen).add(selectedRun.key));
  }, [selectedRun, seenRuns]);
  const projects = useRecentProjects(page);
  useRememberAppPage(page);
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
  // Release smoke tests set a private environment variable and wait for this
  // IPC round trip. Normal app launches take the no-op path in Rust.
  useEffect(() => {
    appClient.markSmokeReady().catch((error) => {
      console.warn("Startup readiness signal failed:", error);
    });
  }, []);

  const batchActive = useBatchActivity();
  const closeProtection = useNativeCloseProtection(navigation.unsavedRef);
  const theme = useAppTheme();
  useAppRouteCommits(
    navigation,
    setup.checkDependencies,
    attention.setTasksAttention,
  );
  useReviewSetupLifecycle(setup, runEventsError);
  const activeRuns = runQueue.runs.filter(
    (run) => run.status === "queued" || run.status === "running",
  );
  const runningReviews = activeRuns.filter(
    (run) => run.status === "running",
  ).length;
  const isRunning = activeRuns.length > 0 || batchActive;
  const hasCurrentRun = selectedRunKey !== null;
  const showRunSetup = page === "main" && selectedRunKey === null;
  // Home's "continue" target: a running review, else the next queued one,
  // else the newest finished one not yet opened.
  const continueRunKey =
    activeRuns.find((run) => run.status === "running")?.key ??
    activeRuns[0]?.key ??
    [...runQueue.runs]
      .reverse()
      .find((run) => run.status === "done" && !seenRuns.has(run.key))?.key ??
    null;
  const reviewLaunch = useReviewLaunch(
    setup,
    { enqueue, enqueueBatch, enqueueRerun },
    navigation,
  );
  const closePipeline = async () => {
    if (!(await router.navigate({ page: "main" }))) return;
    navigation.setWorkflowDirty(false);
    setup.setConfigVersion((version) => version + 1);
  };

  const inspectResearchAttention = async () => {
    const notice = attention.researchNotice;
    if (!notice) return;
    const allowed = await router.navigate({
      page: "workspace",
      projectId: notice.workspaceId,
      surface: "project",
      destination: "checks",
    });
    if (allowed) attention.setResearchNotice(null);
  };

  const openActivityReview = async (id: string) => {
    const queued = runQueue.runs.find(
      (run) => run.runId === id && run.resultAvailable,
    );
    const allowed = await router.navigate(
      queued
        ? { page: "main", runKey: queued.key }
        : { page: "history", runId: id },
    );
    if (!allowed) return;
    if (!queued) navigation.setHistorySourceSelection(null);
  };

  useReviewDependencyWarning(setup, page);
  return {
    navigation,
    setup,
    reviewLaunch,
    attention: { ...attention, inspectResearchAttention },
    appearance: {
      ...theme,
      ...closeProtection,
      navRailWidth,
      setNavRailWidth,
      runSetupWidth,
      setRunSetupWidth,
    },
    projects,
    closePipeline,
    openActivityReview,
    reviewView: {
      state,
      runQueue,
      selectedRun,
      seenRuns,
      runningReviews,
      continueRunKey,
      hasCurrentRun,
      isRunning,
      showRunSetup,
      listenersReady,
      batchActive,
      dismissRuns,
      cancelRun,
      providerLimitNotices: selectedView?.providerLimitNotices ?? [],
      activeRunPlan: selectedRunKey
        ? (reviewLaunch.runPlans[selectedRunKey] ?? null)
        : null,
      stageHistory: selectedView?.stageHistory ?? [],
      runStartedAt: selectedView?.runStartedAt ?? null,
      passTimes: selectedView?.passTimes ?? {},
      reviewRouting: selectedView?.reviewRouting ?? null,
      logs: selectedView?.logs ?? [],
      usage: selectedView?.usage ?? EMPTY_USAGE,
    },
  };
}
