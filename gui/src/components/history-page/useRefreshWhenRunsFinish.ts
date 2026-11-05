import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { RunQueueSnapshot } from "../../lib/types";

/** Reloads history whenever the review queue moves on, so a review appears once
 *  its run exists and its row changes when it ends. Matches queue entries by
 *  their own key: a failed or cancelled entry never carries a run id. */
export default function useRefreshWhenRunsFinish(refresh: () => unknown) {
  useEffect(() => {
    let live = true;
    let unlisten: (() => void) | undefined;
    const progress = new Map<string, string>();
    listen<RunQueueSnapshot>("runs:queue", ({ payload }) => {
      const reached = payload.runs.map(
        (run) => [run.key, `${run.status}:${run.stage}`] as const,
      );
      const moved = reached.some(([key, step]) => progress.get(key) !== step);
      progress.clear();
      reached.forEach(([key, step]) => progress.set(key, step));
      if (live && moved) void refresh();
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
  }, [refresh]);
}
