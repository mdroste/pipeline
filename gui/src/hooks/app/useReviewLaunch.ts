import { useCallback, useRef, useState } from "react";
import { router } from "../../lib/router";
import { appClient } from "../../lib/appClient";
import {
  configForRunPreview,
  type PendingRun,
  type PreparedLaunch,
} from "../../lib/appRunPreparation";
import type { PrimaryInputSelection } from "../../lib/types";
import type { ReviewHandoff } from "../../lib/workbenchTypes";
import type { ExecutionPlanStage } from "../../lib/pipelineHelpers";
import { notify } from "../../components/DialogService";
import type { usePipeline } from "../usePipeline";
import type { ReviewSetup } from "./useReviewSetup";
import type { AppNavigation } from "./useAppNavigation";

type ReviewQueue = Pick<
  ReturnType<typeof usePipeline>,
  "enqueue" | "enqueueBatch" | "enqueueRerun"
>;
export function useReviewLaunch(
  setup: ReviewSetup,
  queue: ReviewQueue,
  navigation: Pick<AppNavigation, "setPage" | "setHistorySourceSelection">,
) {
  const { enqueue, enqueueBatch, enqueueRerun } = queue;
  const { setPage, setHistorySourceSelection } = navigation;
  const {
    paperPath,
    setPaperPath,
    inputSelection,
    setInputSelection,
    inputMode,
    runConfigLoading,
    runProfileConfigSnapshotId,
    parallelOverrides,
    setParallelOverrides,
    setDepsReport,
    setShowDeps,
    setRunConfigError,
    loadRunSetup,
    setPendingRun,
    setSelectionKey,
  } = setup;
  const [runPlans, setRunPlans] = useState<
    Record<string, ExecutionPlanStage[]>
  >({});
  const [preparingRun, setPreparingRun] = useState(false);
  const [runPreview, setRunPreview] = useState<PreparedLaunch | null>(null);
  const launchActive = useRef(false);
  const openRun = useCallback(async (runKey: string) => {
    await router.navigate({ page: "main", runKey });
  }, []);

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
      const overrides = prepared.parallelOverrides
        ? { runParallelOverrides: prepared.parallelOverrides }
        : {};
      // Every document becomes its own queued review; they start as slots
      // free up and each stays reachable from the sidebar.
      const queued = prepared.batchPaths
        ? await enqueueBatch({
            paths: prepared.batchPaths,
            variables: prepared.variables ?? null,
            extraInputs: prepared.extraInputs ?? null,
            expectedProfileConfigSnapshotId:
              prepared.plan.profileConfigSnapshotId,
            ...overrides,
          })
        : [
            await enqueue({
              paperPath: prepared.snapshot.paperPath,
              inputInterpretation:
                prepared.snapshot.inputSelection?.interpretation ?? null,
              diff: false,
              variables: prepared.variables ?? null,
              extraInputs: prepared.extraInputs ?? null,
              expectedProfileSnapshotId: prepared.plan.profileSnapshotId,
              ...overrides,
            }),
          ];
      setRunPlans((plans) => {
        const next = { ...plans };
        for (const run of queued) next[run.key] = prepared.plan.stages;
        return next;
      });
      // Open the new review unless the user moved on while it was queued.
      const current = router.route;
      if (queued[0] && current.page === "main" && !current.runKey)
        router.apply({ page: "main", runKey: queued[0].key });
    } catch (error) {
      setRunConfigError(
        `The review could not be queued: ${error instanceof Error ? error.message : String(error)}`,
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
          "Choose a file or folder, then choose how to review it.",
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
    setPaperPath(null);
    setInputSelection(null);
    setRunPreview(null);
    setHistorySourceSelection(null);
    setPendingRun(null);
    setParallelOverrides(null);
    setSelectionKey((key) => key + 1);
  };

  /** Queue a re-run of a past run and open it. */
  const rerunReview = async (runId: string, onlyFailed: boolean) => {
    try {
      const run = await enqueueRerun(runId, { onlyFailed });
      await openRun(run.key);
    } catch (error) {
      notify(
        `The re-run could not be queued: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  };

  return {
    preparingRun,
    runPreview,
    setRunPreview,
    launch,
    startPreparedLaunch,
    handleGenerate,
    handleWorkspaceReviewHandoff,
    handleNewRun,
    rerunReview,
    openRun,
    runPlans,
  };
}
