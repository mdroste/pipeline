import { describe, expect, it } from "vitest";
import {
  formatAgo,
  formatElapsed,
  orderRuns,
  queuePosition,
  queuedExplanation,
  runStatusLine,
} from "./reviewRuns";
import { queuedRun } from "../test/queuedRunFixture";

describe("review run presentation", () => {
  const runs = [
    queuedRun("old-done", "done", { finishedAt: 5_000 }),
    queuedRun("second-queued", "queued", { queuedAt: 3 }),
    queuedRun("late-runner", "running", { startedAt: 2_000 }),
    queuedRun("new-done", "done", { finishedAt: 9_000 }),
    queuedRun("first-queued", "queued", { queuedAt: 2 }),
    queuedRun("early-runner", "running", { startedAt: 1_000 }),
  ];

  it("orders running, then queued in line, then newest finished", () => {
    expect(orderRuns(runs).map((run) => run.key)).toEqual([
      "early-runner",
      "late-runner",
      "second-queued",
      "first-queued",
      "new-done",
      "old-done",
    ]);
    // Line order is the queue's own order, which the backend keeps.
    expect(queuePosition(runs, "second-queued")).toBe(1);
    expect(queuePosition(runs, "first-queued")).toBe(2);
    expect(queuePosition(runs, "early-runner")).toBeNull();
  });

  it("describes where each review stands in one short line", () => {
    const now = 1_000 + 4 * 60_000 + 7_000;
    expect(runStatusLine(runs[5], runs, now)).toBe("Starting · 4:07");
    expect(
      runStatusLine(
        { ...runs[5], stage: "synthesizing" },
        runs,
        1_000 + 3_725_000,
      ),
    ).toBe("Writing report · 1:02:05");
    expect(runStatusLine(runs[1], runs, now)).toBe("Queued · next");
    expect(runStatusLine(runs[4], runs, now)).toBe("Queued · #2 in line");
    expect(
      runStatusLine({ ...runs[1], waiting: "Batch running" }, runs, now),
    ).toBe("Waiting for another job");
    expect(runStatusLine(runs[3], runs, 9_000 + 5 * 60_000)).toBe(
      "Report ready · 5m ago",
    );
    expect(
      runStatusLine({ ...runs[3], partial: true }, runs, 9_000 + 30_000),
    ).toBe("Finished with errors · just now");
    expect(runStatusLine(queuedRun("x", "failed"), runs, now)).toBe("Failed");
    expect(
      runStatusLine(
        queuedRun("x", "cancelled", { startedAt: null }),
        runs,
        now,
      ),
    ).toBe("Removed from queue");
  });

  it("explains why a queued review has not started", () => {
    expect(queuedExplanation(runs[1], runs, 2)).toBe(
      "It starts as soon as one of the 2 running reviews finishes.",
    );
    expect(queuedExplanation(runs[4], runs, 2)).toBe(
      "It is #2 in line and starts after the 1 ahead of it.",
    );
    const single = [queuedRun("a", "running"), queuedRun("b", "queued")];
    expect(queuedExplanation(single[1], single, 1)).toBe(
      "It starts as soon as the running review finishes.",
    );
    expect(queuedExplanation(single[1], single, 3)).toBe(
      "It is about to start.",
    );
    expect(
      queuedExplanation(
        {
          ...single[1],
          waiting: "A batch, automation, or history operation is running",
        },
        single,
        1,
      ),
    ).toBe(
      "A batch, automation, or history operation is running. This review starts once that job finishes.",
    );
  });

  it("formats clocks and ages", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(65_000)).toBe("1:05");
    expect(formatAgo(3 * 3_600_000)).toBe("3h ago");
    expect(formatAgo(50 * 3_600_000)).toBe("2d ago");
  });
});
