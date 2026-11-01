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
import { confirmRerunRequirements } from "./rerunRequirements";
import type { ReviewSetup } from "./useReviewSetup";
import type { AppNavigation } from "./useAppNavigation";

type ReviewQueue = Pick<
  ReturnType<typeof usePipeline>,
  "enqueue" | "enqueueBatch" | "enqueueRerun"
>;

/** A launch that failed after the workflow setup loaded. When run options
 *  were filled in, they are kept so the options can reopen with them. */
export interface LaunchFailure {
  message: string;
  submitted?: {
    snapshot: PendingRun;
    values: Record<string, string>;
    inputs: Record<string, string>;
  };
}

export function useReviewLaunch(
  setup: ReviewSetup,
  queue: ReviewQueue,
  navigation: Pick<AppNavigation, "setPage" | "clearHistorySourceSelection">,
) {
  const { enqueue, enqueueBatch, enqueueRerun } = queue;
  const { setPage, clearHistorySourceSelection } = navigation;
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
    pendingRun,
    setPendingRun,
    setSelectionKey,
  } = setup;
  const [runPlans, setRunPlans] = useState<
    Record<string, ExecutionPlanStage[]>
  >({});
  const [preparingRun, setPreparingRun] = useState(false);
  const [runPreview, setRunPreview] = useState<PreparedLaunch | null>(null);
  // Separate from the setup error: a refused launch leaves the workflow
  // usable, so it neither blocks Start review nor discards entered values.
  const [launchError, setLaunchError] = useState<LaunchFailure | null>(null);
  const launchActive = useRef(false);
  const openRun = useCallback(async (runKey: string) => {
    await router.navigate({ page: "main", runKey });
  }, []);

  const rejectLaunch = async (
    message: string,
    snapshot: PendingRun,
    variables?: Record<string, string>,
    extraInputs?: Record<string, string>,
  ) => {
    const submitted =
      variables || extraInputs
        ? { snapshot, values: variables ?? {}, inputs: extraInputs ?? {} }
        : undefined;
    setLaunchError({ message, submitted });
    // Reopen the run options with the submitted values, unless the user has
    // moved on from the new-review setup in the meantime.
    const inNewReview = () => {
      const current = router.route;
      return current.page === "main" && !current.runKey;
    };
    if (!submitted || !inNewReview()) return;
    // The refusal may mean the workflow changed after its setup loaded; the
    // same options would then be refused again. Reload the setup first.
    const latest = await loadRunSetup();
    if (!latest || !inNewReview()) return;
    if (latest.profileConfigSnapshotId === snapshot.profileConfigSnapshotId) {
      setPendingRun(snapshot);
      return;
    }
    const pending: PendingRun = {
      ...latest,
      paperPath: snapshot.paperPath,
      inputSelection: snapshot.inputSelection,
    };
    const reopen =
      latest.inputMode === snapshot.inputMode &&
      (latest.variables.length > 0 || latest.inputSlots.length > 0);
    if (!reopen) {
      setLaunchError({
        message:
          "The review changed while you were entering run options. Start it again to use the updated version.",
      });
      return;
    }
    // Carry over the values for options the updated workflow still declares.
    const declared = (values: Record<string, string>, keys: string[]) =>
      Object.fromEntries(
        Object.entries(values).filter(([key]) => keys.includes(key)),
      );
    setLaunchError({
      message:
        "The review changed while you were entering run options. Check them against the updated version, then continue.",
      submitted: {
        snapshot: pending,
        values: declared(
          submitted.values,
          latest.variables.map((variable) => variable.key),
        ),
        inputs: declared(
          submitted.inputs,
          latest.inputSlots.map((slot) => slot.key),
        ),
      },
    });
    setPendingRun(pending);
  };

  const launch = async (
    snapshot: PendingRun,
    variables?: Record<string, string>,
    extraInputs?: Record<string, string>,
  ) => {
    if (launchActive.current) return;
    launchActive.current = true;
    setLaunchError(null);
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
            plan.sequentialAgent,
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
          plan.sequentialAgent,
        ),
        parallelOverrides,
      });
    } catch (error) {
      await rejectLaunch(
        `The review plan could not be prepared: ${
          error instanceof Error ? error.message : String(error)
        }`,
        snapshot,
        variables,
        extraInputs,
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
      await rejectLaunch(
        `The review could not be queued: ${error instanceof Error ? error.message : String(error)}`,
        prepared.snapshot,
        prepared.variables,
        prepared.extraInputs,
      );
    } finally {
      launchActive.current = false;
      setPreparingRun(false);
    }
  };

  const handleGenerate = async () => {
    if (preparingRun || runConfigLoading || !runProfileConfigSnapshotId) return;
    setLaunchError(null);
    setPreparingRun(true);
    try {
      const expectedProfileConfigSnapshotId = runProfileConfigSnapshotId;
      const selectedPath = inputMode === "none" ? "" : (paperPath ?? "");
      if (inputMode !== "none" && !selectedPath) {
        setRunConfigError(
          "This review requires an input. Select it again before running.",
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
          "The active review or settings changed while run inputs were being collected. Review the updated setup and try again.",
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
    setLaunchError(null);
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
    clearHistorySourceSelection();
    setPendingRun(null);
    setLaunchError(null);
    setParallelOverrides(null);
    setSelectionKey((key) => key + 1);
  };

  /** Queue a re-run of a past run and open it. */
  const rerunReview = async (runId: string, onlyFailed: boolean) => {
    try {
      const variables = await confirmRerunRequirements(runId, onlyFailed);
      if (variables === null) return;
      const run = await enqueueRerun(runId, { onlyFailed, variables });
      await openRun(run.key);
    } catch (error) {
      notify(
        `The re-run could not be queued: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  };

  // The refused values and message for the run options now open, when they
  // reopened after that launch failed.
  const launchRetry =
    launchError?.submitted && launchError.submitted.snapshot === pendingRun
      ? { ...launchError.submitted, message: launchError.message }
      : null;

  return {
    preparingRun,
    runPreview,
    setRunPreview,
    launchError: launchError?.message ?? null,
    launchRetry,
    clearLaunchError: () => setLaunchError(null),
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
