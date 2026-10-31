import { useCallback, useEffect, useRef } from "react";
import type { EngineDownloadProgress } from "../lib/types";

export type PhaseStatus = "running" | "done" | "failed";

const PHASES: { id: string; label: string; activity: string }[] = [
  {
    id: "recognition",
    label: "OCR model",
    activity: "Installing the OCR model…",
  },
  { id: "runtime", label: "Python", activity: "Preparing Python…" },
  { id: "packages", label: "Packages", activity: "Installing packages…" },
  {
    id: "models",
    label: "Layout model",
    activity: "Installing the layout model…",
  },
];

/** Sizes use the decimal units of the download estimate on the Install button. */
function formatSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(2)} GB`;
  return `${Math.round(bytes / 1e6)} MB`;
}

function formatSpeed(bytesPerSecond: number): string {
  return bytesPerSecond >= 1e6
    ? `${(bytesPerSecond / 1e6).toFixed(1)} MB/s`
    : `${Math.round(bytesPerSecond / 1e3)} KB/s`;
}

function formatTimeLeft(seconds: number): string {
  if (seconds < 60) return "less than a minute left";
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 60) return `about ${minutes} min left`;
  const remainder = minutes % 60;
  const hours = `about ${Math.floor(minutes / 60)} hr`;
  return remainder ? `${hours} ${remainder} min left` : `${hours} left`;
}

function transferSummary(download: EngineDownloadProgress): string {
  const { received_bytes, total_bytes, bytes_per_second } = download;
  const size = total_bytes
    ? `${formatSize(received_bytes)} of ${formatSize(total_bytes)}`
    : formatSize(received_bytes);
  if (bytes_per_second <= 0) return size;
  const parts = [size, formatSpeed(bytes_per_second)];
  if (total_bytes && total_bytes > received_bytes) {
    parts.push(
      formatTimeLeft((total_bytes - received_bytes) / bytes_per_second),
    );
  }
  return parts.join(" · ");
}

function StepIcon({ status }: { status: PhaseStatus | undefined }) {
  if (status === "running") {
    return (
      <span
        aria-hidden="true"
        className="h-3.5 w-3.5 rounded-full border-2 border-blue-200 border-t-blue-600 animate-spin motion-reduce:animate-none dark:border-blue-900 dark:border-t-blue-400"
      />
    );
  }
  if (status === "done" || status === "failed") {
    return (
      <svg
        aria-hidden="true"
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        className={`h-3.5 w-3.5 ${
          status === "done"
            ? "text-green-600 dark:text-green-400"
            : "text-red-600 dark:text-red-400"
        }`}
      >
        <path
          d={status === "done" ? "M3 8.5 6.5 12 13 4.5" : "M4 4l8 8M12 4l-8 8"}
        />
      </svg>
    );
  }
  return (
    <span
      aria-hidden="true"
      className="h-3.5 w-3.5 rounded-full border-2 border-gray-300 dark:border-gray-600"
    />
  );
}

/**
 * Installation progress for a managed engine: the steps, the current transfer,
 * and the installer's log behind a disclosure.
 */
export default function EngineInstallStatus({
  active,
  phases,
  download,
  logLines,
}: {
  active: boolean;
  phases: Record<string, PhaseStatus>;
  download: EngineDownloadProgress | null;
  logLines: string[];
}) {
  const logRef = useRef<HTMLPreElement>(null);
  const followLog = useCallback(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, []);
  useEffect(followLog, [followLog, logLines]);

  const running = PHASES.find((phase) => phases[phase.id] === "running");
  const percent =
    download?.total_bytes && download.total_bytes > 0
      ? Math.min(
          100,
          Math.floor((download.received_bytes / download.total_bytes) * 100),
        )
      : null;
  const activity = download
    ? `Downloading ${download.label}`
    : (running?.activity ?? "Preparing the installation…");

  return (
    <div className="space-y-3 rounded-lg bg-gray-50 p-3 dark:bg-gray-900">
      <ol className="flex flex-wrap gap-x-4 gap-y-1.5">
        {PHASES.map((phase) => {
          const status = phases[phase.id];
          const shown = status === "running" && !active ? undefined : status;
          return (
            <li
              key={phase.id}
              aria-current={shown === "running" ? "step" : undefined}
              className={`flex items-center gap-1.5 text-xs ${
                shown === "running"
                  ? "font-medium text-gray-900 dark:text-gray-100"
                  : shown === "failed"
                    ? "font-medium text-red-700 dark:text-red-400"
                    : shown === "done"
                      ? "text-gray-600 dark:text-gray-300"
                      : "text-gray-400 dark:text-gray-500"
              }`}
            >
              <StepIcon status={shown} />
              {phase.label}
            </li>
          );
        })}
      </ol>

      {active && (
        <div className="space-y-1.5">
          <div className="flex items-baseline justify-between gap-3 text-xs">
            <span className="min-w-0 truncate font-medium text-gray-800 dark:text-gray-200">
              {activity}
            </span>
            {percent !== null && (
              <span className="shrink-0 tabular-nums text-gray-500 dark:text-gray-400">
                {percent}%
              </span>
            )}
          </div>
          <div
            role="progressbar"
            aria-label={activity}
            aria-valuemin={percent === null ? undefined : 0}
            aria-valuemax={percent === null ? undefined : 100}
            aria-valuenow={percent ?? undefined}
            className="h-1.5 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700"
          >
            <div
              className={`h-full rounded-full transition-[width] duration-300 motion-reduce:transition-none ${
                download?.retrying
                  ? "bg-amber-500"
                  : percent === null
                    ? "animate-pulse bg-blue-400 motion-reduce:animate-none dark:bg-blue-500"
                    : "bg-blue-600 dark:bg-blue-500"
              }`}
              style={{ width: `${percent ?? 100}%` }}
            />
          </div>
          {download && (
            <p
              className={`text-xs tabular-nums ${
                download.retrying
                  ? "text-amber-700 dark:text-amber-300"
                  : "text-gray-500 dark:text-gray-400"
              }`}
            >
              {download.retrying
                ? `Connection interrupted. Reconnecting and continuing from ${formatSize(download.received_bytes)}.`
                : transferSummary(download)}
            </p>
          )}
        </div>
      )}

      {logLines.length > 0 && (
        <details onToggle={followLog}>
          <summary className="cursor-pointer select-none text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200">
            Show details
          </summary>
          <pre
            ref={logRef}
            className="mt-2 max-h-40 overflow-y-auto whitespace-pre-wrap break-words rounded-lg border border-gray-200 bg-white p-2.5 font-mono text-[11px] leading-relaxed text-gray-600 dark:border-gray-700 dark:bg-gray-950 dark:text-gray-400"
          >
            {logLines.join("\n")}
          </pre>
        </details>
      )}
    </div>
  );
}
