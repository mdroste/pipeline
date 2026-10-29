import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { RunQueueSnapshot, RunSummary } from "../../lib/types";

/** Reloads history when a review listed as running finishes in the review
 *  queue, so its row gains its report and actions. */
export default function useRefreshWhenRunsFinish(
  runs: RunSummary[],
  refresh: () => unknown,
) {
  const runningIds = runs
    .filter((run) => run.status === "running")
    .map((run) => run.run_id)
    .join("\0");

  useEffect(() => {
    if (!runningIds) return;
    const running = new Set(runningIds.split("\0"));
    let live = true;
    let unlisten: (() => void) | undefined;
    listen<RunQueueSnapshot>("runs:queue", ({ payload }) => {
      const finished = payload.runs.some(
        (run) =>
          run.runId !== null &&
          running.has(run.runId) &&
          run.status !== "running",
      );
      if (live && finished) void refresh();
    })
      .then((off) => {
        if (live) unlisten = off;
        else off();
      })
      .catch(() => undefined);
    return () => {
      live = false;
      unlisten?.();
    };
  }, [runningIds, refresh]);
}
