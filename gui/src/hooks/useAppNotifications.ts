import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { WorkbenchEvent } from "../lib/workbenchTypes";
import type { BatchJob, QueuedRun } from "../lib/types";
import {
  automationNotice,
  deliverNotice,
  workspaceNotice,
  type AppNotice,
} from "../lib/appNotifications";

/** Subscribe at the shell, so navigating away from a feature doesn't mute it. */
export function useAppNotifications(runs: QueuedRun[]) {
  const seen = useRef(new Map<string, number>());
  const previousStatus = useRef(new Map<string, QueuedRun["status"]>());
  const deliver = (notice: AppNotice | null) => {
    if (!notice) return;
    const now = Date.now();
    if (now - (seen.current.get(notice.id) ?? -Infinity) < 5000) return;
    seen.current.set(notice.id, now);
    if (seen.current.size > 500)
      seen.current.delete(seen.current.keys().next().value!);
    void deliverNotice(notice);
  };
  // Announce each review that finishes while this window watches it. A
  // review first seen already finished (after a reload) is not announced.
  useEffect(() => {
    const previous = previousStatus.current;
    previousStatus.current = new Map(runs.map((run) => [run.key, run.status]));
    for (const run of runs) {
      const before = previous.get(run.key);
      if (before !== "queued" && before !== "running") continue;
      if (run.status === "done")
        deliver({
          id: `review:${run.key}`,
          kind: run.partial ? "failure" : "completion",
          title: run.partial
            ? "Review finished with errors"
            : "Review complete",
          body: run.partial
            ? `${run.label}: open the report to inspect the failed steps.`
            : `${run.label} is ready to read.`,
        });
      if (run.status === "failed")
        deliver({
          id: `review-error:${run.key}`,
          kind: "failure",
          title: "Review failed",
          body: `${run.label}: open it from Reviews to inspect the error.`,
        });
    }
  }, [runs]);
  useEffect(() => {
    let live = true;
    let jobs: BatchJob[] = [];
    const handle = (notice: AppNotice | null) => {
      if (live) deliver(notice);
    };
    const pending = [
      listen<WorkbenchEvent>("workbench:event", ({ payload }) =>
        handle(workspaceNotice(payload)),
      ),
      ...["tasks", "missions", "discovery"].map((source) =>
        listen<{
          id: string;
          state: string;
          reason?: string | null;
          revision?: number;
        }>(`${source}:notice`, ({ payload }) =>
          handle(automationNotice(source, payload)),
        ),
      ),
      listen<{ workspaceId: string; checkId: string }>(
        "workbench-research-attention",
        ({ payload }) =>
          handle({
            id: `research:${payload.workspaceId}:${payload.checkId}`,
            kind: "attention",
            title: "Research check needs attention",
            body: "Open the project’s checks to inspect the change.",
          }),
      ),
      listen<BatchJob[]>("batch:progress", ({ payload }) => {
        jobs = payload;
      }),
      listen("batch:done", () => {
        if (
          jobs.length &&
          jobs.every(
            (job) => job.status === "cancelled" || job.status === "pending",
          )
        )
          return;
        const failed = jobs.some((job) => job.status === "failed");
        handle({
          id: `batch:${Date.now()}`,
          kind: failed ? "failure" : "completion",
          title: failed ? "Batch finished with errors" : "Batch finished",
          body: "Open Reviews to inspect completed reports and any failures.",
        });
      }),
    ].map((registration) => registration.catch(() => () => {}));
    return () => {
      live = false;
      pending.forEach((p) => void p.then((off) => off()).catch(() => {}));
    };
  }, []);
}
