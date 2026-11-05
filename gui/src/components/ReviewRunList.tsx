import { useEffect, useState } from "react";
import { Icon } from "../ui/icons";
import type { QueuedRun } from "../lib/types";
import { isActiveRun, orderRuns, runStatusLine } from "../lib/reviewRuns";

/** Already-opened finished reviews kept in the rail; unopened ones always
 *  show, and everything stays in Review history. */
const SEEN_FINISHED_SHOWN = 2;

interface Props {
  runs: QueuedRun[];
  limit: number;
  /** The review open in the main area, if any. */
  activeRunKey: string | null;
  /** Finished reviews the user has not opened yet. */
  unseen: ReadonlySet<string>;
  onOpen: (key: string) => void;
  onCancel: (run: QueuedRun) => void;
  onDismiss: (key?: string) => void;
  onConfigure?: () => void;
}

/** A clock that ticks only while something on screen is counting. */
function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active]);
  return now;
}

function StatusGlyph({ run, unseen }: { run: QueuedRun; unseen: boolean }) {
  switch (run.status) {
    case "running":
      return (
        <span
          aria-hidden="true"
          className="block h-3.5 w-3.5 animate-spin rounded-full border-2 border-blue-500/25 border-t-blue-500 dark:border-blue-400/25 dark:border-t-blue-400"
        />
      );
    case "queued":
      return (
        <span
          aria-hidden="true"
          className="block h-3.5 w-3.5 rounded-full border-[1.5px] border-dashed border-gray-400 dark:border-neutral-500"
        />
      );
    case "done":
      return run.partial ? (
        <Icon name="warning" className="h-4 w-4 text-amber-500" />
      ) : unseen ? (
        <span
          aria-hidden="true"
          className="block h-2.5 w-2.5 rounded-full bg-emerald-500 ring-4 ring-emerald-500/15"
        />
      ) : (
        <Icon
          name="check"
          className="h-4 w-4 text-emerald-600 dark:text-emerald-400"
        />
      );
    case "failed":
      return <Icon name="warning" className="h-4 w-4 text-red-500" />;
    case "cancelled":
      return (
        <Icon
          name="stop"
          className="h-4 w-4 text-gray-400 dark:text-neutral-500"
        />
      );
  }
}

function RunRow({
  run,
  runs,
  now,
  active,
  unseen,
  onOpen,
  onCancel,
  onDismiss,
}: {
  run: QueuedRun;
  runs: QueuedRun[];
  now: number;
  active: boolean;
  unseen: boolean;
  onOpen: () => void;
  onCancel: () => void;
  onDismiss: () => void;
}) {
  const status = runStatusLine(run, runs, now);
  const live = isActiveRun(run);
  const actionLabel =
    run.status === "running"
      ? `Cancel review of ${run.label}`
      : run.status === "queued"
        ? `Remove ${run.label} from the queue`
        : `Dismiss ${run.label}`;
  return (
    <li className="group relative">
      <button
        type="button"
        aria-current={active ? "page" : undefined}
        onClick={onOpen}
        title={`${run.label}\n${status}${run.inputPath ? `\n${run.inputPath}` : ""}`}
        data-run-status={run.status}
        className={`flex w-full items-start gap-2.5 rounded-lg py-1 pl-2.5 pr-7 text-left transition-colors
                    focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 ${
                      active
                        ? "bg-white shadow-sm ring-1 ring-gray-200/80 dark:bg-neutral-800 dark:ring-neutral-700"
                        : "hover:bg-white/70 dark:hover:bg-neutral-800/70"
                    }`}
      >
        <span className="mt-[2px] flex h-4 w-4 shrink-0 items-center justify-center">
          <StatusGlyph run={run} unseen={unseen} />
        </span>
        <span className="min-w-0 flex-1">
          <span
            className={`block truncate text-[13px] leading-5 ${
              unseen || live
                ? "font-medium text-gray-900 dark:text-neutral-100"
                : "text-gray-600 dark:text-neutral-400"
            }`}
          >
            {run.label}
          </span>
          <span
            className={`block truncate text-[11px] leading-4 ${
              run.status === "running"
                ? "tabular-nums text-blue-700 dark:text-blue-300"
                : run.status === "failed"
                  ? "text-red-600 dark:text-red-400"
                  : "text-gray-500 dark:text-neutral-500"
            }`}
          >
            {status}
            {unseen && <span className="sr-only"> · not opened yet</span>}
          </span>
        </span>
      </button>
      <button
        type="button"
        onClick={live ? onCancel : onDismiss}
        aria-label={actionLabel}
        title={
          run.status === "running"
            ? "Cancel review"
            : run.status === "queued"
              ? "Remove from queue"
              : "Dismiss from list"
        }
        className="absolute right-1 top-1 rounded-md p-1 text-gray-400 opacity-0 transition hover:bg-gray-200/80 hover:text-gray-800
                   focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400
                   group-focus-within:opacity-100 group-hover:opacity-100 dark:hover:bg-neutral-700 dark:hover:text-neutral-100"
      >
        <Icon
          name={run.status === "running" ? "stop" : "close"}
          className="h-3.5 w-3.5"
        />
      </button>
    </li>
  );
}

/** Reviews in the queue: what is running, what waits, and what just
 *  finished. Each row returns to that review. */
export default function ReviewRunList({
  runs,
  limit,
  activeRunKey,
  unseen,
  onOpen,
  onCancel,
  onDismiss,
  onConfigure,
}: Props) {
  const running = runs.filter((run) => run.status === "running").length;
  const queued = runs.filter((run) => run.status === "queued").length;
  const now = useNow(running > 0 || runs.some((run) => run.status === "done"));
  if (runs.length === 0) return null;
  const ordered = orderRuns(runs);
  const active = ordered.filter(isActiveRun);
  const finished = ordered.filter((run) => !isActiveRun(run));
  const seenFinished = finished.filter((run) => !unseen.has(run.key));
  const shown = [
    ...active,
    ...finished.filter(
      (run) =>
        unseen.has(run.key) ||
        seenFinished.indexOf(run) < SEEN_FINISHED_SHOWN ||
        run.key === activeRunKey,
    ),
  ];
  return (
    <div className="space-y-0.5 pt-1">
      <div className="flex items-center gap-1 pl-2.5 pr-1 text-[11px] text-gray-500 dark:text-neutral-500">
        <p role="status" className="min-w-0 flex-1 truncate tabular-nums">
          {running + queued > 0
            ? `${running} of ${limit} running${queued > 0 ? ` · ${queued} queued` : ""}`
            : "Recently finished"}
        </p>
        {onConfigure && (
          <button
            type="button"
            onClick={onConfigure}
            aria-label="Change how many reviews run at once"
            title="Change how many reviews run at once"
            className="rounded p-0.5 text-gray-400 hover:bg-gray-200/80 hover:text-gray-700 focus-visible:outline-none focus-visible:ring-2
                       focus-visible:ring-gray-400 dark:hover:bg-neutral-700 dark:hover:text-neutral-200"
          >
            <Icon name="settings" className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
      <ul
        aria-label="Reviews in progress and recently finished"
        className="space-y-0.5"
      >
        {shown.map((run) => (
          <RunRow
            key={run.key}
            run={run}
            runs={runs}
            now={now}
            active={run.key === activeRunKey}
            unseen={unseen.has(run.key)}
            onOpen={() => onOpen(run.key)}
            onCancel={() => onCancel(run)}
            onDismiss={() => onDismiss(run.key)}
          />
        ))}
      </ul>
      {finished.length > 0 && (
        <button
          type="button"
          onClick={() => onDismiss()}
          className="ml-2.5 rounded px-0 py-0.5 text-[11px] text-gray-400 hover:text-gray-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-neutral-500 dark:hover:text-neutral-200"
        >
          Clear finished
          {finished.length > 1 ? ` (${finished.length})` : ""}
        </button>
      )}
    </div>
  );
}
