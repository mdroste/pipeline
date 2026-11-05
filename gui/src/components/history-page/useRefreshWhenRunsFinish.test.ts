import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { QueuedRun } from "../../lib/types";
import useRefreshWhenRunsFinish from "./useRefreshWhenRunsFinish";

const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

// The fields `QueuedRun` serializes from the queue. `run_id` is only set when
// a review succeeds, so failed and cancelled entries always carry null.
const queued = (patch: Partial<QueuedRun>): QueuedRun => ({
  key: "first",
  kind: "review",
  label: "Paper",
  inputPath: "/paper.pdf",
  profileName: "Review",
  status: "running",
  stage: "reviewing",
  stageLabel: "Reviewing",
  waiting: null,
  queuedAt: 1,
  startedAt: 2,
  finishedAt: null,
  runId: null,
  error: null,
  partial: false,
  resultAvailable: false,
  ...patch,
});

async function mounted() {
  const refresh = vi.fn();
  renderHook(() => useRefreshWhenRunsFinish(refresh));
  await waitFor(() => expect(listen).toHaveBeenCalled());
  const emit = (runs: QueuedRun[]) =>
    act(async () => listen.mock.lastCall![1]({ payload: { limit: 2, runs } }));
  return { refresh, emit };
}

describe("useRefreshWhenRunsFinish", () => {
  beforeEach(() => {
    listen.mockReset();
    listen.mockResolvedValue(() => {});
  });

  it.each(["cancelled", "failed"] as const)(
    "refreshes when a review ends %s with no run id",
    async (status) => {
      const { refresh, emit } = await mounted();
      await emit([queued({})]);
      refresh.mockClear();
      await emit([queued({ status, error: "Stopped" })]);
      expect(refresh).toHaveBeenCalledTimes(1);
    },
  );

  it("refreshes for a review that started after history loaded", async () => {
    const { refresh, emit } = await mounted();
    await emit([queued({ stage: "extracting" })]);
    refresh.mockClear();
    await emit([
      queued({ stage: "extracting" }),
      queued({ key: "second", status: "queued", stage: null }),
    ]);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("refreshes when a review reaches the stage where its run exists", async () => {
    const { refresh, emit } = await mounted();
    await emit([queued({ stage: "extracting" })]);
    refresh.mockClear();
    await emit([queued({ stage: "reviewing" })]);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("does not reload for a repeated snapshot or a new stage label", async () => {
    const { refresh, emit } = await mounted();
    await emit([queued({})]);
    refresh.mockClear();
    await emit([queued({ stageLabel: "Reviewing step 2" })]);
    expect(refresh).not.toHaveBeenCalled();
  });
});
