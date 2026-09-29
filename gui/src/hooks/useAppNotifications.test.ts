import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { PipelineState } from "./usePipeline";
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
  const view = renderHook(() => useAppNotifications({ kind: "idle" }));
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
  renderHook(() => useAppNotifications({ kind: "idle" }));
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
it("notifies on a Review terminal transition without replaying old state", () => {
  const initial = { kind: "extracting" } as PipelineState;
  const view = renderHook(
    ({ state }: { state: PipelineState }) => useAppNotifications(state),
    { initialProps: { state: initial } },
  );
  view.rerender({ state: { kind: "error", message: "failed call" } });
  expect(deliverNotice).toHaveBeenCalledWith(
    expect.objectContaining({ kind: "failure", title: "Review failed" }),
  );
  view.rerender({ state: { kind: "error", message: "failed call" } });
  expect(deliverNotice).toHaveBeenCalledTimes(1);
});

it("delivers discovery selection and completion while its page is unmounted", () => {
  renderHook(() => useAppNotifications({ kind: "idle" }));
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
