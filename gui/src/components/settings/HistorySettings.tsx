import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirmDialog } from "../DialogService";

import type { Settings } from "../../lib/types";

import InfoButton from "../InfoButton";

export function RunRetention({
  settings,
  setSettings,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
}) {
  const [usage, setUsage] = useState<{ count: number; bytes: number } | null>(
    null,
  );
  const [purging, setPurging] = useState(false);
  const [purgeError, setPurgeError] = useState<string | null>(null);
  const [purgeResult, setPurgeResult] = useState<string | null>(null);

  const loadUsage = async () => {
    try {
      setUsage(
        await invoke<{ count: number; bytes: number }>("runs_disk_usage"),
      );
    } catch {
      setUsage(null);
    }
  };
  useEffect(() => {
    void loadUsage();
  }, []);

  const fmtBytes = (n: number) =>
    n >= 1_000_000_000
      ? (n / 1_000_000_000).toFixed(1) + " GB"
      : n >= 1_000_000
        ? (n / 1_000_000).toFixed(0) + " MB"
        : (n / 1_000).toFixed(0) + " KB";

  const purgeNow = async () => {
    setPurging(true);
    setPurgeError(null);
    setPurgeResult(null);
    let purgeStarted = false;
    try {
      const preview = await invoke<{
        delete_count: number;
        delete_bytes: number;
        remaining_count: number;
        remaining_bytes: number;
        preview_token: string;
      }>("preview_purge_runs", {
        keep: settings.max_saved_runs,
        maxBytes: settings.max_saved_run_bytes,
      });
      if (preview.delete_count === 0) {
        setPurgeResult(
          "No completed reports were beyond the configured limits.",
        );
        await loadUsage();
        return;
      }
      const confirmed = await confirmDialog(
        "Move reports to Trash?\n\n" +
          `This will move ${preview.delete_count} completed report${
            preview.delete_count === 1 ? "" : "s"
          } (${fmtBytes(preview.delete_bytes)}). ` +
          `${preview.remaining_count} report${preview.remaining_count === 1 ? "" : "s"} ` +
          `(${fmtBytes(preview.remaining_bytes)}) will remain.\n\n` +
          "Reports remain recoverable from Trash until deliberately deleted there.",
        {
          title: "Apply retention policy",
          confirmLabel: "Move to Trash",
          destructive: true,
        },
      );
      if (!confirmed) return;

      purgeStarted = true;
      const removed = await invoke<number>("purge_runs", {
        keep: settings.max_saved_runs,
        maxBytes: settings.max_saved_run_bytes,
        previewToken: preview.preview_token,
      });
      setPurgeResult(
        removed === 0
          ? "No completed reports were beyond the configured limits."
          : `Moved ${removed} completed report${removed === 1 ? "" : "s"} to Trash.`,
      );
      await loadUsage();
    } catch (error) {
      setPurgeError(
        `${
          purgeStarted
            ? "Report history could not be moved to Trash"
            : "The retention preview could not be loaded; no reports were moved"
        }: ${error instanceof Error ? error.message : String(error)}`,
      );
    } finally {
      setPurging(false);
    }
  };

  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5">
        <span className="text-sm font-medium text-gray-700 dark:text-neutral-300">
          Report history retention
        </span>
        <InfoButton label="Report history retention">
          Past reports and their artifacts are stored under{" "}
          <code>~/.pipeline/runs/</code>. After each report, Pipeline removes
          the oldest completed reports until both limits hold. Set a limit to 0
          to disable it.
        </InfoButton>
        {usage && (
          <span className="ml-auto text-xs font-normal text-gray-500 dark:text-neutral-400">
            {usage.count} report{usage.count === 1 ? "" : "s"} ·{" "}
            {fmtBytes(usage.bytes)}
          </span>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <input
          aria-label="Maximum saved reports"
          type="number"
          min={0}
          value={settings.max_saved_runs}
          onChange={(e) =>
            setSettings({
              ...settings,
              max_saved_runs: Math.max(0, parseInt(e.target.value, 10) || 0),
            })
          }
          className="w-24 py-2 px-3 border border-gray-300 dark:border-neutral-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-neutral-800 dark:text-neutral-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20"
        />
        <span className="text-xs text-gray-500">reports and</span>
        <input
          aria-label="Report history size limit in GB"
          type="number"
          min={0}
          max={1000}
          step={1}
          value={Math.round(
            (settings.max_saved_run_bytes ?? 5_000_000_000) / 1_000_000_000,
          )}
          onChange={(e) =>
            setSettings({
              ...settings,
              max_saved_run_bytes:
                Math.max(0, parseInt(e.target.value, 10) || 0) * 1_000_000_000,
            })
          }
          className="w-24 py-2 px-3 border border-gray-300 dark:border-neutral-600 rounded-lg text-sm text-gray-900 bg-white dark:bg-neutral-800 dark:text-neutral-200 focus:outline-none focus:ring-2 focus:ring-gray-900/20"
        />
        <span className="text-xs text-gray-500">GB</span>
        <button
          onClick={purgeNow}
          disabled={
            purging ||
            (settings.max_saved_runs === 0 &&
              settings.max_saved_run_bytes === 0)
          }
          className="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-neutral-600 text-gray-700 dark:text-neutral-300 hover:bg-gray-50 dark:hover:bg-neutral-800 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          title={
            settings.max_saved_runs === 0 && settings.max_saved_run_bytes === 0
              ? "Set a limit above 0 to purge"
              : "Delete reports beyond the limits now"
          }
        >
          {purging ? "Purging…" : "Purge now"}
        </button>
      </div>
      {purgeError && (
        <p role="alert" className="mt-2 text-sm text-red-600 dark:text-red-400">
          {purgeError}
        </p>
      )}
      {purgeResult && (
        <p
          role="status"
          className="mt-2 text-sm text-green-700 dark:text-green-400"
        >
          {purgeResult}
        </p>
      )}
    </div>
  );
}

/* ── Shared UI Components ────────────────────────────────────────── */
