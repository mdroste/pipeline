import { describe, expect, it } from "vitest";
import { emptyRunView, syncWithQueue } from "./runViews";
import { queuedRun } from "../test/queuedRunFixture";

describe("syncWithQueue", () => {
  it("places a failure seen only through the queue at its last stage", () => {
    const view = syncWithQueue(
      undefined,
      queuedRun("draft", "failed", { stage: "dispatching", error: "boom" }),
    );
    expect(view.state).toEqual({
      kind: "error",
      message: "boom",
      failedAt: "dispatching",
    });
    expect(view.terminal).toBe(true);

    const unknown = syncWithQueue(undefined, queuedRun("x", "failed"));
    expect(unknown.state).toEqual({
      kind: "error",
      message: "The review failed.",
      failedAt: undefined,
    });
  });

  it("starts the clock from the queue when a run begins without events", () => {
    const view = syncWithQueue(
      emptyRunView(),
      queuedRun("draft", "running", { startedAt: 42 }),
    );
    expect(view.state).toEqual({ kind: "extracting" });
    expect(view.runStartedAt).toBe(42);
  });
});
