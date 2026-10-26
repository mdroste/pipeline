import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { QueuedRun } from "../lib/types";
import { queuedRun as fixture } from "../test/queuedRunFixture";
const queuedRun = (status: QueuedRun["status"], key = "run-a") =>
  fixture(key, status);
import { useAppNotifications } from "./useAppNotifications";
const handlers = vi.hoisted(
  () => new Map<string, (event: { payload: unknown }) => void>(),
);
const deliverNotice = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const off = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    (name: string, callback: (event: { payload: unknown }) => void) => {
      handlers.set(name, callback);
      return Promise.resolve(off);
    },
  ),
}));
vi.mock("../lib/appNotifications", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/appNotifications")>()),
  deliverNotice,
}));
beforeEach(() => {
  handlers.clear();
  vi.clearAllMocks();
});
const emit = (name: string, payload: unknown) =>
  act(() => handlers.get(name)!({ payload }));
it("listens at the shell, deduplicates repeated notices, and releases listeners", async () => {
  const view = renderHook(() => useAppNotifications([]));
  emit("tasks:notice", { id: "task", state: "finished" });
  emit("tasks:notice", { id: "task", state: "finished" });
  expect(deliverNotice).toHaveBeenCalledTimes(1);
  emit("workbench:event", { kind: "serverRequest", epoch: 1, requestId: 42 });
  expect(deliverNotice).toHaveBeenLastCalledWith(
    expect.objectContaining({ kind: "attention" }),
  );
  view.unmount();
  await waitFor(() => expect(off).toHaveBeenCalledTimes(7));
});
it("classifies failed batches as failures and ignores cancelled batches", () => {
  renderHook(() => useAppNotifications([]));
  emit("batch:progress", [{ status: "failed" }, { status: "done" }]);
  emit("batch:done", null);
  expect(deliverNotice).toHaveBeenLastCalledWith(
    expect.objectContaining({
      kind: "failure",
      title: "Batch finished with errors",
    }),
  );
  deliverNotice.mockClear();
  emit("batch:progress", [{ status: "cancelled" }]);
  emit("batch:done", null);
  expect(deliverNotice).not.toHaveBeenCalled();
});
it("notifies once per Review that finishes, naming it, without replaying old state", () => {
  const view = renderHook(
    ({ runs }: { runs: QueuedRun[] }) => useAppNotifications(runs),
    {
      initialProps: {
        runs: [queuedRun("running"), queuedRun("done", "already-done")],
      },
    },
  );
  expect(deliverNotice).not.toHaveBeenCalled();
  view.rerender({
    runs: [queuedRun("failed"), queuedRun("done", "already-done")],
  });
  expect(deliverNotice).toHaveBeenCalledWith(
    expect.objectContaining({
      kind: "failure",
      title: "Review failed",
      body: expect.stringContaining("run-a.pdf"),
    }),
  );
  view.rerender({
    runs: [queuedRun("failed"), queuedRun("done", "already-done")],
  });
  expect(deliverNotice).toHaveBeenCalledTimes(1);
  view.rerender({
    runs: [queuedRun("running", "run-b"), queuedRun("queued", "run-c")],
  });
  view.rerender({
    runs: [queuedRun("done", "run-b"), queuedRun("cancelled", "run-c")],
  });
  expect(deliverNotice).toHaveBeenCalledTimes(2);
  expect(deliverNotice).toHaveBeenLastCalledWith(
    expect.objectContaining({
      kind: "completion",
      title: "Review complete",
      body: "run-b.pdf is ready to read.",
    }),
  );
});

it("delivers discovery selection and completion while its page is unmounted", () => {
  renderHook(() => useAppNotifications([]));
  emit("discovery:notice", {
    id: "portfolio",
    state: "awaitingSelection",
    revision: 4,
  });
  emit("discovery:notice", {
    id: "portfolio",
    state: "awaitingSelection",
    revision: 4,
  });
  expect(deliverNotice).toHaveBeenCalledTimes(1);
  expect(deliverNotice).toHaveBeenLastCalledWith(
    expect.objectContaining({ kind: "attention", discoveryId: "portfolio" }),
  );
  emit("discovery:notice", {
    id: "portfolio",
    state: "completed",
    revision: 8,
  });
  expect(deliverNotice).toHaveBeenLastCalledWith(
    expect.objectContaining({ kind: "completion", discoveryId: "portfolio" }),
  );
});
