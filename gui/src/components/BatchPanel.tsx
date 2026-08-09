import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { BatchJob, WatchStatus, InputSlot, VarSpec, DepsReport } from "../lib/types";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";

interface Props {
  onClose: () => void;
  onOpenRun: (runId: string) => void;
  showClose?: boolean;
  /** Execution plan already loaded by the app shell for the active workflow. */
  preloadedSetup?: BatchSetupEnvelope | null;
}

export interface BatchSetupEnvelope {
  profileId: string;
  profileConfigSnapshotId: string;
  profileSnapshotId: string;
  inputMode: string;
  variables: VarSpec[];
  inputSlots: InputSlot[];
  readiness: DepsReport;
  stages: ExecutionPlanStage[];
}

function fmtDuration(secs: number): string {
  if (secs <= 0) return "";
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

function statusColor(status: string): string {
  switch (status) {
    case "done":
      return "text-green-600 dark:text-green-400";
    case "failed":
      return "text-red-600 dark:text-red-400";
    case "running":
      return "text-blue-600 dark:text-blue-400 animate-pulse";
    case "cancelled":
      return "text-gray-500 dark:text-gray-400";
    default:
      return "text-gray-500 dark:text-gray-400";
  }
}

function representativeBatchPath(paths: string[]): string | null {
  return paths.find((path) => !/\.(?:tex|docx)$/i.test(path)) ?? paths[0] ?? null;
}

/** Run one profile over many inputs, one at a time. Each input becomes a normal
 *  run in history; progress arrives via batch:progress events. */
export default function BatchPanel({
  onClose,
  onOpenRun,
  showClose = true,
  preloadedSetup = null,
}: Props) {
  const initialSpecs = preloadedSetup?.variables ?? [];
  // Inputs staged before a batch starts.
  const [staged, setStaged] = useState<string[]>([]);
  // Live job list from the backend once a batch is running.
  const [jobs, setJobs] = useState<BatchJob[]>([]);
  const [watch, setWatch] = useState<WatchStatus | null>(null);
  const [inputMode, setInputMode] = useState(preloadedSetup?.inputMode || "document");
  const [variables, setVariables] = useState<VarSpec[]>(initialSpecs);
  const [variableValues, setVariableValues] = useState<Record<string, string>>(
    Object.fromEntries(initialSpecs.map((spec) => [spec.key, spec.default ?? ""])),
  );
  const [inputSlots, setInputSlots] = useState<InputSlot[]>(preloadedSetup?.inputSlots ?? []);
  const [extraInputs, setExtraInputs] = useState<Record<string, string>>({});
  const [profileConfigSnapshotId, setProfileConfigSnapshotId] = useState<string | null>(
    preloadedSetup?.profileConfigSnapshotId ?? null,
  );
  const [error, setError] = useState<string | null>(null);
  const [errorCanRetrySetup, setErrorCanRetrySetup] = useState(false);
  const [profileLoading, setProfileLoading] = useState(!preloadedSetup);
  const [listenersReady, setListenersReady] = useState(false);
  const [setupAttempt, setSetupAttempt] = useState(0);
  const [preparingStart, setPreparingStart] = useState(false);
  const profileRequest = useRef(0);

  const running = jobs.some((j) => j.status === "running" || j.status === "pending");
  const documentMode = inputMode === "document";

  const applySetup = useCallback((setup: BatchSetupEnvelope) => {
    const specs = setup.variables ?? [];
    setInputMode(setup.inputMode || "document");
    setVariables(specs);
    setVariableValues(Object.fromEntries(specs.map((spec) => [spec.key, spec.default ?? ""])));
    setInputSlots(setup.inputSlots ?? []);
    setExtraInputs({});
    setProfileConfigSnapshotId(setup.profileConfigSnapshotId);
  }, []);

  // The app shell may finish its startup plan while this lazy page is being
  // mounted. Apply that plan immediately; status/listener synchronization can
  // continue without holding the workflow options behind a loading message.
  useEffect(() => {
    if (!preloadedSetup || setupAttempt !== 0) return;
    applySetup(preloadedSetup);
    setProfileLoading(false);
  }, [applySetup, preloadedSetup, setupAttempt]);

  useEffect(() => {
    let live = true;
    let unlisteners: Array<() => void> = [];
    const request = ++profileRequest.current;
    const warmSetup = setupAttempt === 0 ? preloadedSetup : null;
    setProfileLoading(!warmSetup);
    setListenersReady(false);
    setError(null);
    setErrorCanRetrySetup(false);
    const listenerSetup = Promise.allSettled([
      listen<BatchJob[]>("batch:progress", (e) => setJobs(e.payload)),
      listen("batch:done", () => invoke<BatchJob[]>("get_batch_status").then(setJobs).catch(() => {})),
      listen<WatchStatus>("watch:status", (e) => setWatch(e.payload)),
    ]);
    listenerSetup
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
          return null;
        }
        unlisteners = registered;
        setListenersReady(true);
        // Read status only after every event listener is active. Otherwise a
        // run can start between the initial status read and listener setup.
        return Promise.all([
          invoke<BatchJob[]>("get_batch_status"),
          invoke<WatchStatus>("get_watch_status"),
          warmSetup
            ? Promise.resolve(warmSetup)
            : invoke<BatchSetupEnvelope>("get_execution_plan", {
                variables: null,
                extraInputs: null,
                expectedProfileConfigSnapshotId: null,
                diff: false,
                paperPath: null,
              }),
        ]);
      })
      .then((result) => {
        if (!live || request !== profileRequest.current || !result) return;
        const [loadedJobs, loadedWatch, setup] = result;
        setJobs(loadedJobs);
        setWatch(loadedWatch);
        // The warm setup is already editable. Do not reset a user's variable
        // or named-input choices when the slower status reads finish.
        if (!warmSetup) applySetup(setup);
        setError(null);
        setErrorCanRetrySetup(false);
        setProfileLoading(false);
      })
      .catch((caught) => {
        if (!live || request !== profileRequest.current) return;
        setError(`Batch setup could not be loaded: ${caught instanceof Error ? caught.message : String(caught)}`);
        setErrorCanRetrySetup(true);
        setProfileLoading(false);
      });
    return () => {
      live = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [applySetup, preloadedSetup, setupAttempt]);

  const prepareCurrentSetup = useCallback(async (
    paperPath?: string | null,
  ): Promise<BatchSetupEnvelope> => {
    if (!profileConfigSnapshotId) {
      throw new Error("The active workflow setup is not ready. Refresh it and review the options.");
    }
    const setup = await invoke<BatchSetupEnvelope>("get_execution_plan", {
      variables: variableValues,
      extraInputs,
      expectedProfileConfigSnapshotId: profileConfigSnapshotId,
      diff: false,
      paperPath: paperPath ?? null,
    });
    if (setup.inputMode !== "document") {
      throw new Error("The active workflow no longer accepts document inputs. Refresh the setup.");
    }
    if (!setup.readiness.ready) {
      const missing = setup.readiness.deps
        .filter((dependency) =>
          dependency.required && (
            !dependency.found ||
            dependency.authenticated === false ||
            (
              dependency.cli_auth_status === "unknown" &&
              dependency.authenticated !== true &&
              !/gemini/i.test(dependency.name)
            )
          ),
        )
        .map((dependency) =>
          `${dependency.name}${dependency.hint ? `: ${dependency.hint}` : ""}`,
        )
        .join("; ");
      throw new Error(
        missing
          ? `The active workflow is not ready. Missing: ${missing}.`
          : "The active workflow is not ready. Refresh dependencies and try again.",
      );
    }
    return setup;
  }, [extraInputs, profileConfigSnapshotId, variableValues]);

  const startWatch = useCallback(async () => {
    setError(null);
    setErrorCanRetrySetup(false);
    setPreparingStart(true);
    try {
      const missing = inputSlots.find((slot) => slot.required && !extraInputs[slot.key]);
      if (missing) {
        setError(`Select the required input “${missing.label || missing.key}” before watching.`);
        return;
      }
      let dir: string | string[] | null;
      try {
        dir = await open({ directory: true });
      } catch (caught) {
        const message = caught instanceof Error ? caught.message : String(caught);
        setError(`Could not open the folder picker for watching: ${message}`);
        return;
      }
      if (typeof dir !== "string") return;
      const setup = await prepareCurrentSetup(null);
      await invoke("start_watch", {
        folder: dir,
        variables: variableValues,
        extraInputs,
        expectedProfileConfigSnapshotId: setup.profileConfigSnapshotId,
      });
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(`Watching could not be started: ${message}`);
      setErrorCanRetrySetup(true);
    } finally {
      setPreparingStart(false);
    }
  }, [extraInputs, inputSlots, prepareCurrentSetup, variableValues]);

  const pickExtraInput = useCallback(async (slot: InputSlot) => {
    setError(null);
    setErrorCanRetrySetup(false);
    try {
      const picked = await open({
        directory: slot.mode === "folder",
        multiple: false,
        ...(slot.mode === "folder"
          ? {}
          : { filters: [{ name: slot.label || "Input", extensions: ["pdf", "tex", "docx"] }] }),
      });
      if (typeof picked === "string") {
        setExtraInputs((current) => ({ ...current, [slot.key]: picked }));
      }
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(
        `Could not open the ${slot.mode === "folder" ? "folder" : "file"} picker for “${
          slot.label || slot.key
        }”: ${message}`,
      );
    }
  }, []);

  const pickVariableFile = useCallback(async (spec: VarSpec) => {
    setError(null);
    setErrorCanRetrySetup(false);
    try {
      const picked = await open({
        directory: false,
        multiple: false,
        filters: [{ name: spec.label || "Input", extensions: ["pdf", "tex", "txt", "md", "docx", "json", "csv"] }],
      });
      if (typeof picked === "string") {
        setVariableValues((current) => ({ ...current, [spec.key]: picked }));
      }
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(
        `Could not open the file picker for “${spec.label || spec.key}”: ${message}`,
      );
    }
  }, []);

  const stopWatch = useCallback(async () => {
    setError(null);
    setErrorCanRetrySetup(false);
    try {
      const stopped = await invoke<WatchStatus>("stop_watch");
      setWatch(stopped);
    } catch (caught) {
      setError(
        `Watching could not be stopped: ${
          caught instanceof Error ? caught.message : String(caught)
        }`,
      );
    }
  }, []);

  const addFiles = useCallback(async () => {
    setError(null);
    setErrorCanRetrySetup(false);
    try {
      const picked = await open({ multiple: true, filters: [{ name: "Papers", extensions: ["pdf", "tex", "docx"] }] });
      if (Array.isArray(picked)) setStaged((prev) => [...new Set([...prev, ...picked])]);
      else if (typeof picked === "string") setStaged((prev) => [...new Set([...prev, picked])]);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(`Could not open the file picker: ${message}`);
    }
  }, []);

  const addFolder = useCallback(async () => {
    setError(null);
    setErrorCanRetrySetup(false);
    let dir: string | string[] | null;
    try {
      dir = await open({ directory: true });
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(`Could not open the folder picker: ${message}`);
      return;
    }
    if (typeof dir !== "string") return;
    try {
      const files = await invoke<string[]>("list_input_files", { dir });
      if (files.length === 0) setError("No PDF, LaTeX, or Word files found directly in that folder.");
      setStaged((prev) => [...new Set([...prev, ...files])]);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(`Could not add files from the selected folder: ${message}`);
    }
  }, []);

  const removeStaged = (p: string) => setStaged((prev) => prev.filter((x) => x !== p));

  const start = useCallback(async () => {
    setError(null);
    setErrorCanRetrySetup(false);
    setPreparingStart(true);
    try {
      const missing = inputSlots.find((slot) => slot.required && !extraInputs[slot.key]);
      if (missing) {
        setError(`Select the required input “${missing.label || missing.key}” before starting.`);
        return;
      }
      const setup = await prepareCurrentSetup(representativeBatchPath(staged));
      await invoke("start_batch", {
        paths: staged,
        variables: variableValues,
        extraInputs,
        expectedProfileConfigSnapshotId: setup.profileConfigSnapshotId,
      });
      setStaged([]);
    } catch (e) {
      setError(
        `${e instanceof Error ? e.message : String(e)} Refresh the workflow setup and review its options before retrying.`,
      );
      setErrorCanRetrySetup(true);
    } finally {
      setPreparingStart(false);
    }
  }, [extraInputs, inputSlots, prepareCurrentSetup, staged, variableValues]);

  const cancel = useCallback(async () => {
    setError(null);
    setErrorCanRetrySetup(false);
    try {
      await invoke("cancel_batch");
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(`Batch cancellation failed: ${message}`);
    }
  }, []);

  const done = jobs.filter((j) => j.status === "done").length;
  const failed = jobs.filter((j) => j.status === "failed").length;
  const batchSnapshot = jobs.find((job) => job.profile_id || job.profile_snapshot_id);
  const optionSummary = [
    variables.length > 0
      ? `variables: ${variables.map((spec) => spec.label || spec.key).join(", ")}`
      : "",
    inputSlots.length > 0
      ? `named inputs: ${inputSlots.map((slot) => slot.label || slot.key).join(", ")}`
      : "",
  ].filter(Boolean).join(" · ");

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 px-6 py-3 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Batch run</h2>
        <span className="text-xs text-gray-500 dark:text-gray-400">
          Runs the active profile over each input, one at a time.
        </span>
        {showClose && (
          <button
            type="button"
            onClick={onClose}
            className="ml-auto text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
            aria-label="Close batch"
          >
            ✕
          </button>
        )}
      </div>

      <div className="flex-1 overflow-auto p-4 space-y-4">
        {error && (
          <div role="alert" className="flex items-start gap-3 p-3 bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 rounded text-sm text-red-700 dark:text-red-400">
            <span className="flex-1">{error}</span>
            {errorCanRetrySetup && (
              <button
                type="button"
                onClick={() => setSetupAttempt((attempt) => attempt + 1)}
                className="shrink-0 rounded border border-red-300 px-2 py-1 text-xs font-medium dark:border-red-800"
              >
                Retry setup
              </button>
            )}
          </div>
        )}

        {profileLoading && (
          <p role="status" className="text-sm text-gray-500 dark:text-gray-400">
            Loading active workflow…
          </p>
        )}

        {!profileLoading && !documentMode && (
          <div role="note" className="rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-900 dark:border-amber-900 dark:bg-amber-950/35 dark:text-amber-200">
            <p className="font-medium">Batch and watch require a document-input workflow.</p>
            <p className="mt-1 text-xs leading-5">
              The active workflow uses {inputMode === "none" ? "no primary input" : "a folder input"}.
              Switch to a document workflow before queuing files.
            </p>
          </div>
        )}

        {documentMode && (variables.length > 0 || inputSlots.length > 0) && !running && !watch?.active && (
          <div className="border border-gray-200 dark:border-gray-800 rounded-lg p-3 space-y-2">
            <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">
              Shared run options
            </h3>
            <p className="text-xs text-gray-500 dark:text-gray-400">
              These values are captured once and supplied to every batch or watch job.
            </p>
            {variables.map((spec) => {
              const id = `batch-variable-${spec.key}`;
              return (
                <div key={spec.key} className="grid grid-cols-[10rem_minmax(0,1fr)_auto] items-center gap-2 text-sm">
                  <label htmlFor={id} className="truncate text-gray-700 dark:text-gray-300">
                    {spec.label || spec.key}
                  </label>
                  {spec.kind === "choice" && spec.choices?.length ? (
                    <select
                      id={id}
                      value={variableValues[spec.key] ?? ""}
                      onChange={(event) => setVariableValues((current) => ({ ...current, [spec.key]: event.target.value }))}
                      className="min-w-0 rounded border border-gray-300 bg-white px-2 py-1 text-sm text-gray-900 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
                    >
                      {spec.choices.map((choice) => <option key={choice} value={choice}>{choice}</option>)}
                    </select>
                  ) : (
                    <input
                      id={id}
                      value={variableValues[spec.key] ?? ""}
                      onChange={(event) => setVariableValues((current) => ({ ...current, [spec.key]: event.target.value }))}
                      className="min-w-0 rounded border border-gray-300 bg-white px-2 py-1 text-sm text-gray-900 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
                    />
                  )}
                  {spec.kind === "file" ? (
                    <button
                      type="button"
                      onClick={() => pickVariableFile(spec)}
                      className="rounded border border-gray-300 px-2.5 py-1 text-gray-700 dark:border-gray-600 dark:text-gray-300"
                    >
                      Choose…
                    </button>
                  ) : <span />}
                </div>
              );
            })}
            {inputSlots.map((slot) => (
              <div key={slot.key} className="flex items-center gap-2 text-sm">
                <span className="w-40 truncate text-gray-700 dark:text-gray-300">
                  {slot.label || slot.key}{slot.required ? " *" : ""}
                </span>
                <button
                  type="button"
                  onClick={() => pickExtraInput(slot)}
                  className="px-2.5 py-1 rounded border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300"
                >
                  Choose…
                </button>
                <span className="flex-1 truncate text-xs text-gray-500" title={extraInputs[slot.key]}>
                  {extraInputs[slot.key]?.split(/[\\/]/).pop() || "Not selected"}
                </span>
              </div>
            ))}
          </div>
        )}

        {/* Staging area (before start) */}
        {documentMode && !profileLoading && !running && (
          <div>
            <div className="flex items-center gap-2 mb-2">
              <button
                type="button"
                onClick={addFiles}
                className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
              >
                Add files…
              </button>
              <button
                type="button"
                onClick={addFolder}
                className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
              >
                Add folder…
              </button>
              <button
                type="button"
                onClick={start}
                disabled={
                  staged.length === 0 ||
                  !listenersReady ||
                  !profileConfigSnapshotId ||
                  preparingStart
                }
                className="ml-auto px-4 py-1.5 text-sm rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
              >
                Start batch ({staged.length})
              </button>
            </div>
            {staged.length > 0 ? (
              <div className="space-y-1">
                {staged.map((p) => (
                  <div
                    key={p}
                    className="flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300 px-2 py-1 rounded hover:bg-gray-50 dark:hover:bg-gray-800"
                  >
                    <span className="flex-1 truncate" title={p}>
                      {p.split("/").pop()}
                    </span>
                    <button
                      type="button"
                      onClick={() => removeStaged(p)}
                      aria-label={`Remove ${p.split(/[\\/]/).pop()}`}
                      className="text-xs text-gray-400 hover:text-red-500"
                    >
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-sm text-gray-500 dark:text-gray-400 px-2">Add files or a folder of PDF, LaTeX, or Word papers to queue.</p>
            )}
          </div>
        )}

        {/* Watch a folder */}
        {documentMode && !profileLoading && <div className="border border-gray-200 dark:border-gray-800 rounded-lg p-3">
          <div className="flex items-center gap-2">
            <div className="flex-1 min-w-0">
              <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">Watch a folder</h3>
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                {watch?.active
                  ? <>Watching <span className="font-mono">{watch.folder}</span> — new files run automatically.</>
                  : "Auto-run the active profile on files added to a folder."}
              </p>
              <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">
                Watching pauses while another pipeline run is active, then resumes automatically.
              </p>
              {watch?.paused && (
                <p role="status" className="mt-1 text-xs font-medium text-amber-700 dark:text-amber-300">
                  Paused while another run is active.
                </p>
              )}
              {watch?.error && (
                <p role="alert" className="mt-1 text-xs font-medium text-red-700 dark:text-red-300">
                  Watch stopped: {watch.error}
                </p>
              )}
              {watch?.active && (watch.profile_id || watch.profile_snapshot_id) && (
                <p className="mt-1 break-all text-[11px] text-gray-500 dark:text-gray-400">
                  Captured profile <span className="font-mono">{watch.profile_id || "unknown"}</span>
                  {watch.profile_snapshot_id && (
                    <> · snapshot <span className="font-mono">{watch.profile_snapshot_id}</span></>
                  )}
                  {optionSummary && <> · {optionSummary}</>}
                </p>
              )}
            </div>
            {watch?.active ? (
              <button
                type="button"
                onClick={stopWatch}
                className="px-3 py-1.5 text-sm rounded-lg border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-950"
              >
                Stop watching
              </button>
            ) : (
              <button
                type="button"
                onClick={startWatch}
                disabled={!listenersReady || !profileConfigSnapshotId || preparingStart}
                className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:cursor-not-allowed disabled:opacity-40"
              >
                Watch folder…
              </button>
            )}
          </div>
          {watch && watch.processed.length > 0 && (
            <div className="mt-2 space-y-1">
              <p className="text-xs text-gray-500 dark:text-gray-400">
                Processed {watch.processed_total ?? watch.processed.length}
                {(watch.failed_total ?? 0) > 0 && <span className="text-red-500"> · {watch.failed_total} failed</span>}
                {(watch.processed_total ?? watch.processed.length) > watch.processed.length && <> · showing latest {watch.processed.length}</>}
              </p>
              {watch.processed.map((j, i) => (
                <div key={i} className="flex items-center gap-2 text-xs">
                  <span className="flex-1 truncate text-gray-700 dark:text-gray-300" title={j.path}>{j.name}</span>
                  <span className={statusColor(j.status)}>{j.status}</span>
                  {j.run_id && (
                    <button onClick={() => onOpenRun(j.run_id!)} className="text-blue-600 dark:text-blue-400 hover:underline">
                      Open
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>}

        {/* Running / finished job list */}
        {jobs.length > 0 && (
          <div>
            {batchSnapshot && (
              <p className="mb-2 break-all rounded bg-gray-50 px-3 py-2 text-[11px] text-gray-500 dark:bg-gray-800/50 dark:text-gray-400">
                Captured profile <span className="font-mono">{batchSnapshot.profile_id || "unknown"}</span>
                {batchSnapshot.profile_snapshot_id && (
                  <> · snapshot <span className="font-mono">{batchSnapshot.profile_snapshot_id}</span></>
                )}
                {optionSummary && <> · {optionSummary}</>}
              </p>
            )}
            <div className="flex items-center justify-between mb-2">
              <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">
                Jobs {done + failed}/{jobs.length} done
                {failed > 0 && <span className="text-red-500"> · {failed} failed</span>}
              </h3>
              {running && (
                <button
                  onClick={cancel}
                  className="px-3 py-1 text-xs rounded-lg border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-950"
                >
                  Cancel batch
                </button>
              )}
            </div>
            <div className="space-y-1">
              {jobs.map((j, i) => (
                <div
                  key={i}
                  className="flex items-center gap-3 text-sm px-3 py-2 border border-gray-200 dark:border-gray-800 rounded-lg bg-white dark:bg-gray-900"
                >
                  <span className="flex-1 truncate text-gray-800 dark:text-gray-200" title={j.path}>
                    {j.name}
                  </span>
                  {j.duration_secs > 0 && (
                    <span className="text-xs text-gray-500 dark:text-gray-400 tabular-nums">{fmtDuration(j.duration_secs)}</span>
                  )}
                  <span className={`text-xs font-medium ${statusColor(j.status)}`}>{j.status}</span>
                  {j.run_id && (
                    <button
                      onClick={() => onOpenRun(j.run_id!)}
                      className="text-xs text-blue-600 dark:text-blue-400 hover:underline"
                    >
                      Open
                    </button>
                  )}
                  {j.error && (
                    <span className="text-xs text-red-500 max-w-[16rem] truncate" title={j.error}>
                      {j.error}
                    </span>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
