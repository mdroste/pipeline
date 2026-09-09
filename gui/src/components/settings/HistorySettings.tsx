import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { confirmDialog } from "../DialogService";

import type { Settings } from "../../lib/types";

import InfoButton from "../InfoButton";
import ValidatedNumber from "./ValidatedNumber";

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
          Past reports and their artifacts are stored in your active research
          data folder. After each report, Pipeline moves to Trash the oldest
          completed reports until both limits hold. Set a limit to 0 to disable
          it.
        </InfoButton>
        {usage && (
          <span className="ml-auto text-xs font-normal text-gray-500 dark:text-neutral-400">
            {usage.count} report{usage.count === 1 ? "" : "s"} ·{" "}
            {fmtBytes(usage.bytes)}
          </span>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <ValidatedNumber
          label="Maximum saved reports"
          value={settings.max_saved_runs}
          max={1000000}
          onChange={(value) =>
            setSettings({ ...settings, max_saved_runs: value })
          }
        />
        <span className="text-xs text-gray-500">reports and</span>
        <ValidatedNumber
          label="Report history size limit in GB"
          value={
            (settings.max_saved_run_bytes ?? 5_000_000_000) / 1_000_000_000
          }
          max={1000}
          step={0.1}
          onChange={(value) =>
            setSettings({
              ...settings,
              max_saved_run_bytes: Math.round(value * 1_000_000_000),
            })
          }
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
              ? "Set a limit above 0 to review cleanup"
              : "Preview reports beyond these limits"
          }
        >
          {purging ? "Preparing cleanup…" : "Review cleanup…"}
        </button>
      </div>
      <p className="settings-row-description mt-3">
        Set either limit to 0 for unlimited. Reports beyond these limits move to
        Trash after a review finishes. Review cleanup previews the exact reports
        before making changes.
      </p>
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
