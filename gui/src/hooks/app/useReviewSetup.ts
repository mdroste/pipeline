import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { appClient } from "../../lib/appClient";
import { router } from "../../lib/router";
import {
  plannedInterpretation,
  type PendingRun,
  type RunProfileSnapshot,
} from "../../lib/appRunPreparation";
import type {
  DepsReport,
  PipelineConfig,
  PrimaryInputSelection,
  RunParallelOverrides,
} from "../../lib/types";
import type { AppPage } from "../../components/NavRail";

export function useReviewSetup() {
  const [paperPath, setPaperPath] = useState<string | null>(null);
  const [inputSelection, setInputSelection] =
    useState<PrimaryInputSelection | null>(null);
  const [depsReport, setDepsReport] = useState<DepsReport | null>(null);
  const [depsLoading, setDepsLoading] = useState(true);
  const [depsError, setDepsError] = useState<string | null>(null);
  const [configVersion, setConfigVersion] = useState(0);
  const [selectionKey, setSelectionKey] = useState(0);
  const [profileChanges, setProfileChanges] = useState(0);
  // Active profile's input mode ("document" | "folder" | "none") — refetched
  // whenever the pipeline config may have changed. "none" workflows can run
  // without selecting an input.
  const [inputMode, setInputMode] = useState<string>("document");
  const [runProfileConfigSnapshotId, setRunProfileConfigSnapshotId] = useState<
    string | null
  >(null);
  const [runConfigLoading, setRunConfigLoading] = useState(true);
  const [runConfigError, setRunConfigError] = useState<string | null>(null);
  const [parallelOverrides, setParallelOverrides] =
    useState<RunParallelOverrides | null>(null);
  // Variables and extra input slots the active profile declares; both drive
  // the pre-run options modal.
  const [pendingRun, setPendingRun] = useState<PendingRun | null>(null);
  const selectedInputRef = useRef({ inputMode, paperPath, inputSelection });
  selectedInputRef.current = { inputMode, paperPath, inputSelection };
  const parallelOverridesRef = useRef(parallelOverrides);
  parallelOverridesRef.current = parallelOverrides;
  const [showDeps, setShowDeps] = useState(false);

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

  const runDependencyCheck = useCallback(
    async (includeInput: boolean): Promise<DepsReport | null> => {
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
          paperPath:
            selected.inputMode === "none"
              ? ""
              : includeInput
                ? selected.paperPath
                : null,
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
    },
    [],
  );
  const checkDependencies = useCallback(
    () => runDependencyCheck(true),
    [runDependencyCheck],
  );

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
      setDepsLoading(true);
      setDepsError(null);
      setRunConfigLoading(true);
      setConfigVersion((version) => version + 1);
      setProfileChanges((changes) => changes + 1);
    },
    [inputMode],
  );

  // Refreshes setup status without repeating the preflight of a selected input,
  // which the launch preview runs. The cleared report stands for "checking"
  // until then, not for a missing tool.
  useEffect(() => {
    if (profileChanges) void runDependencyCheck(false);
  }, [profileChanges, runDependencyCheck]);

  return {
    paperPath,
    setPaperPath,
    inputSelection,
    setInputSelection,
    depsReport,
    setDepsReport,
    depsLoading,
    depsError,
    configVersion,
    setConfigVersion,
    selectionKey,
    setSelectionKey,
    inputMode,
    runProfileConfigSnapshotId,
    runConfigLoading,
    setRunConfigLoading,
    runConfigError,
    setRunConfigError,
    parallelOverrides,
    setParallelOverrides,
    pendingRun,
    setPendingRun,
    loadRunSetup,
    checkDependencies,
    handleProfileChange,
    showDeps,
    setShowDeps,
  };
}

export type ReviewSetup = ReturnType<typeof useReviewSetup>;

const STARTUP_DEPENDENCY_CHECK_DELAY = 2500;

// Kept at the shell's original lifecycle position relative to route commits.
export function useReviewSetupLifecycle(
  setup: ReviewSetup,
  runEventsError: string | null,
) {
  const { configVersion, loadRunSetup, checkDependencies, setRunConfigError } =
    setup;
  useEffect(() => {
    void loadRunSetup();
  }, [configVersion, loadRunSetup]);

  const startupDependencyCheckStarted = useRef(false);
  useEffect(() => {
    if (startupDependencyCheckStarted.current) return;
    const start = () => {
      startupDependencyCheckStarted.current = true;
      void checkDependencies();
    };
    // Starting a review needs the result now. Elsewhere it only colors the
    // status dot, so it waits for the first page to finish loading.
    if (router.getSnapshot().route.page === "main") return start();
    const timer = setTimeout(start, STARTUP_DEPENDENCY_CHECK_DELAY);
    return () => clearTimeout(timer);
  }, [checkDependencies]);

  useEffect(() => {
    if (runEventsError) setRunConfigError(runEventsError);
  }, [runEventsError]);
}

export function useReviewDependencyWarning(setup: ReviewSetup, page: AppPage) {
  const { depsReport, setShowDeps } = setup;
  // Auto-show missing dependencies only in the new-review setup. `main` with a
  // run key shows a queued, running, or finished review, whose workflow this
  // report does not describe. If the user has already navigated (most
  // importantly, to Settings), do not let a late readiness response cover
  // that destination with this modal.
  const { route } = useSyncExternalStore(router.subscribe, router.getSnapshot);
  const setupView = page === "main" && route.page === "main" && !route.runKey;
  // Show each unresolved report once. A recheck with the same result (for
  // example after leaving Settings) does not reopen a dismissed warning.
  const shownReport = useRef<string | null>(null);
  useEffect(() => {
    if (!depsReport) return;
    if (depsReport.ready) {
      shownReport.current = null;
      return;
    }
    if (!setupView) return;
    const signature = JSON.stringify(depsReport.deps);
    if (signature === shownReport.current) return;
    shownReport.current = signature;
    setShowDeps(true);
  }, [depsReport, setupView]);
}
