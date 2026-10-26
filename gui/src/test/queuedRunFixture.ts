import type { QueuedRun } from "../lib/types";

/** A run-queue entry for tests: `<key>.pdf`, started at t=1000 unless queued. */
export function queuedRun(
  key: string,
  status: QueuedRun["status"] = "queued",
  extra: Partial<QueuedRun> = {},
): QueuedRun {
  return {
    key,
    kind: "review",
    label: `${key}.pdf`,
    inputPath: `/papers/${key}.pdf`,
    profileName: "Review",
    status,
    stage: null,
    stageLabel: null,
    waiting: null,
    queuedAt: 1,
    startedAt: status === "queued" ? null : 1000,
    finishedAt: null,
    runId: null,
    error: null,
    partial: false,
    resultAvailable: status === "done",
    ...extra,
  };
}
