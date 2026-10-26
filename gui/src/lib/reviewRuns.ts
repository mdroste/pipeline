// Presentation rules for reviews in the run queue: the sidebar list, the
// queued-review panel, and the Home "continue" card all describe runs the
// same way.

import type { QueuedRun } from "./types";

export function isActiveRun(run: QueuedRun): boolean {
  return run.status === "queued" || run.status === "running";
}

/** Running first (oldest start first), then queued in queue order, then
 *  finished (newest first). */
export function orderRuns(runs: QueuedRun[]): QueuedRun[] {
  const running = runs
    .filter((run) => run.status === "running")
    .sort((a, b) => (a.startedAt ?? 0) - (b.startedAt ?? 0));
  const queued = runs.filter((run) => run.status === "queued");
  const finished = runs
    .filter((run) => !isActiveRun(run))
    .sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0));
  return [...running, ...queued, ...finished];
}

/** 1-based place of a queued review in line, or null when not queued. */
export function queuePosition(runs: QueuedRun[], key: string): number | null {
  const index = runs
    .filter((run) => run.status === "queued")
    .findIndex((run) => run.key === key);
  return index < 0 ? null : index + 1;
}

const STAGE_WORDS: Record<string, string> = {
  extracting: "Reading input",
  orienting: "Orienting",
  dispatching: "Reviewing",
  merging: "Merging",
  synthesizing: "Writing report",
};

export function stageWord(stage: string | null): string {
  return (stage && STAGE_WORDS[stage]) || "Starting";
}

/** "4:07" or "1:02:09". */
export function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, "0");
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${seconds}`
    : `${minutes}:${seconds}`;
}

/** "just now", "5m ago", "3h ago", "2d ago". */
export function formatAgo(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

function ordinal(position: number): string {
  return position === 1 ? "next" : `#${position} in line`;
}

/** One short line saying where a review stands. */
export function runStatusLine(
  run: QueuedRun,
  runs: QueuedRun[],
  now: number,
): string {
  switch (run.status) {
    case "running":
      return `${stageWord(run.stage)} · ${formatElapsed(now - (run.startedAt ?? now))}`;
    case "queued": {
      if (run.waiting) return "Waiting for another job";
      const position = queuePosition(runs, run.key) ?? 1;
      return `Queued · ${ordinal(position)}`;
    }
    case "done":
      return `${run.partial ? "Finished with errors" : "Report ready"} · ${formatAgo(now - (run.finishedAt ?? now))}`;
    case "failed":
      return "Failed";
    case "cancelled":
      return run.startedAt === null ? "Removed from queue" : "Cancelled";
  }
}

/** Why a queued review has not started, in a full sentence. */
export function queuedExplanation(
  run: QueuedRun,
  runs: QueuedRun[],
  limit: number,
): string {
  if (run.waiting)
    return `${run.waiting.replace(/\.$/, "")}. This review starts once that job finishes.`;
  const running = runs.filter(
    (candidate) => candidate.status === "running",
  ).length;
  const position = queuePosition(runs, run.key) ?? 1;
  if (position > 1)
    return `It is #${position} in line and starts after the ${position - 1} ahead of it.`;
  if (running < limit) return "It is about to start.";
  return running === 1
    ? "It starts as soon as the running review finishes."
    : `It starts as soon as one of the ${running} running reviews finishes.`;
}
