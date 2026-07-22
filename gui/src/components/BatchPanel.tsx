import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { BatchJob, WatchStatus, InputSlot, PipelineConfig } from "../lib/types";

interface Props {
  onClose: () => void;
  onOpenRun: (runId: string) => void;
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
      return "text-gray-400";
    default:
      return "text-gray-400 dark:text-gray-500";
  }
}

/** Run one profile over many inputs, one at a time. Each input becomes a normal
 *  run in history; progress arrives via batch:progress events. */
export default function BatchPanel({ onClose, onOpenRun }: Props) {
  // Inputs staged before a batch starts.
  const [staged, setStaged] = useState<string[]>([]);
  // Live job list from the backend once a batch is running.
  const [jobs, setJobs] = useState<BatchJob[]>([]);
  const [watch, setWatch] = useState<WatchStatus | null>(null);
  const [inputSlots, setInputSlots] = useState<InputSlot[]>([]);
  const [extraInputs, setExtraInputs] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  const running = jobs.some((j) => j.status === "running" || j.status === "pending");

  useEffect(() => {
    invoke<BatchJob[]>("get_batch_status").then(setJobs).catch(() => {});
    invoke<WatchStatus>("get_watch_status").then(setWatch).catch(() => {});
    invoke<PipelineConfig>("get_pipeline_config")
      .then((config) => setInputSlots(config.extraction?.extra_inputs ?? []))
      .catch(() => {});
    const unlisteners: Array<Promise<() => void>> = [
      listen<BatchJob[]>("batch:progress", (e) => setJobs(e.payload)),
      listen("batch:done", () => invoke<BatchJob[]>("get_batch_status").then(setJobs).catch(() => {})),
      listen<WatchStatus>("watch:status", (e) => setWatch(e.payload)),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, []);

  const startWatch = useCallback(async () => {
    setError(null);
    try {
      const missing = inputSlots.find((slot) => slot.required && !extraInputs[slot.key]);
      if (missing) {
        setError(`Select the required input “${missing.label || missing.key}” before watching.`);
        return;
      }
      const dir = await open({ directory: true });
      if (typeof dir !== "string") return;
      await invoke("start_watch", { folder: dir, extraInputs });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [extraInputs, inputSlots]);

  const pickExtraInput = useCallback(async (slot: InputSlot) => {
    try {
      const picked = await open({
        directory: slot.mode === "folder",
        multiple: false,
        ...(slot.mode === "folder"
          ? {}
          : { filters: [{ name: slot.label || "Input", extensions: ["pdf", "tex", "txt", "md", "docx"] }] }),
      });
      if (typeof picked === "string") {
        setExtraInputs((current) => ({ ...current, [slot.key]: picked }));
      }
    } catch {
      /* cancelled */
    }
  }, []);

  const stopWatch = useCallback(async () => {
    try {
      await invoke("stop_watch");
    } catch {
      /* ignore */
    }
  }, []);

  const addFiles = useCallback(async () => {
    try {
      const picked = await open({ multiple: true, filters: [{ name: "Papers", extensions: ["pdf", "tex"] }] });
      if (Array.isArray(picked)) setStaged((prev) => [...new Set([...prev, ...picked])]);
      else if (typeof picked === "string") setStaged((prev) => [...new Set([...prev, picked])]);
    } catch {
      /* cancelled */
    }
  }, []);

  const addFolder = useCallback(async () => {
    try {
      const dir = await open({ directory: true });
      if (typeof dir !== "string") return;
      const files = await invoke<string[]>("list_input_files", { dir });
      if (files.length === 0) setError("No PDF or LaTeX files found directly in that folder.");
      setStaged((prev) => [...new Set([...prev, ...files])]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const removeStaged = (p: string) => setStaged((prev) => prev.filter((x) => x !== p));

  const start = useCallback(async () => {
    setError(null);
    try {
      const missing = inputSlots.find((slot) => slot.required && !extraInputs[slot.key]);
      if (missing) {
        setError(`Select the required input “${missing.label || missing.key}” before starting.`);
        return;
      }
      await invoke("start_batch", { paths: staged, extraInputs });
      setStaged([]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [extraInputs, inputSlots, staged]);

  const cancel = useCallback(async () => {
    try {
      await invoke("cancel_batch");
    } catch {
      /* ignore */
    }
  }, []);

  const done = jobs.filter((j) => j.status === "done").length;
  const failed = jobs.filter((j) => j.status === "failed").length;

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-3 px-6 py-3 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">Batch run</h2>
        <span className="text-xs text-gray-400 dark:text-gray-500">
          Runs the active profile over each input, one at a time.
        </span>
        <button
          onClick={onClose}
          className="ml-auto text-gray-400 hover:text-gray-600 dark:hover:text-gray-300"
          title="Close"
        >
          ✕
        </button>
      </div>

      <div className="flex-1 overflow-auto p-4 space-y-4">
        {error && (
          <div className="p-3 bg-red-50 dark:bg-red-950 border border-red-200 dark:border-red-800 rounded text-sm text-red-700 dark:text-red-400">
            {error}
          </div>
        )}

        {inputSlots.length > 0 && !running && !watch?.active && (
          <div className="border border-gray-200 dark:border-gray-800 rounded-lg p-3 space-y-2">
            <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">
              Shared profile inputs
            </h3>
            <p className="text-xs text-gray-500 dark:text-gray-400">
              These files are supplied to every batch or watch job.
            </p>
            {inputSlots.map((slot) => (
              <div key={slot.key} className="flex items-center gap-2 text-sm">
                <span className="w-40 truncate text-gray-700 dark:text-gray-300">
                  {slot.label || slot.key}{slot.required ? " *" : ""}
                </span>
                <button
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
        {!running && (
          <div>
            <div className="flex items-center gap-2 mb-2">
              <button
                onClick={addFiles}
                className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
              >
                Add files…
              </button>
              <button
                onClick={addFolder}
                className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
              >
                Add folder…
              </button>
              <button
                onClick={start}
                disabled={staged.length === 0}
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
                      onClick={() => removeStaged(p)}
                      className="text-xs text-gray-400 hover:text-red-500"
                    >
                      ✕
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-sm text-gray-400 px-2">Add files or a folder of PDFs/LaTeX to queue.</p>
            )}
          </div>
        )}

        {/* Watch a folder */}
        <div className="border border-gray-200 dark:border-gray-800 rounded-lg p-3">
          <div className="flex items-center gap-2">
            <div className="flex-1 min-w-0">
              <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">Watch a folder</h3>
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                {watch?.active
                  ? <>Watching <span className="font-mono">{watch.folder}</span> — new files run automatically.</>
                  : "Auto-run the active profile on files added to a folder."}
              </p>
            </div>
            {watch?.active ? (
              <button
                onClick={stopWatch}
                className="px-3 py-1.5 text-sm rounded-lg border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-950"
              >
                Stop watching
              </button>
            ) : (
              <button
                onClick={startWatch}
                className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
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
        </div>

        {/* Running / finished job list */}
        {jobs.length > 0 && (
          <div>
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
                    <span className="text-xs text-gray-400 tabular-nums">{fmtDuration(j.duration_secs)}</span>
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
