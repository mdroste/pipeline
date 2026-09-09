import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { WorkbenchEvent } from "../lib/workbenchTypes";
import type { PipelineState } from "./usePipeline";
import type { BatchJob } from "../lib/types";
import {
  automationNotice,
  deliverNotice,
  workspaceNotice,
  type AppNotice,
} from "../lib/appNotifications";

/** Subscribe at the shell, so navigating away from a feature doesn't mute it. */
export function useAppNotifications(state: PipelineState) {
  const seen = useRef(new Map<string, number>());
  const previousRun = useRef(state.kind);
  const deliver = (notice: AppNotice | null) => {
    if (!notice) return;
    const now = Date.now();
    if (now - (seen.current.get(notice.id) ?? -Infinity) < 5000) return;
    seen.current.set(notice.id, now);
    if (seen.current.size > 500)
      seen.current.delete(seen.current.keys().next().value!);
    void deliverNotice(notice);
  };
  useEffect(() => {
    const previous = previousRun.current;
    previousRun.current = state.kind;
    if (previous === state.kind || previous === "idle") return;
    if (state.kind === "done")
      deliver({
        id: `review:${state.runId ?? Date.now()}`,
        kind: state.report.failed_steps?.length ? "failure" : "completion",
        title: state.report.failed_steps?.length
          ? "Review finished with errors"
          : "Review complete",
        body: state.report.failed_steps?.length
          ? "Open the report to inspect the failed steps."
          : "Your report is ready to read.",
      });
    if (state.kind === "error" && !/cancelled|canceled/i.test(state.message))
      deliver({
        id: `review-error:${Date.now()}`,
        kind: "failure",
        title: "Review failed",
        body: "Open Reviews to inspect the error.",
      });
  }, [state]);
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
      ...["tasks", "missions"].map((source) =>
        listen<{ id: string; state: string; reason?: string | null }>(
          `${source}:notice`,
          ({ payload }) => handle(automationNotice(source, payload)),
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
