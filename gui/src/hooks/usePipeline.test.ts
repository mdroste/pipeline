import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { usePipeline } from "./usePipeline";

// Capture registered event handlers so tests can emit synthetic events.
const handlers: Record<string, (event: { payload: unknown }) => void> = {};
const unlisten = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((name: string, cb: (event: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(unlisten);
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => new Promise(() => {})),
}));

function emitLog(line: string) {
  handlers["pipeline:log"]({ payload: { line } });
}

describe("usePipeline log buffering", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("flushes buffered log lines into state", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => {
      emitLog("line one");
      emitLog("line two");
    });

    // The flush interval runs every 100ms.
    await waitFor(() =>
      expect(result.current.logs).toEqual(["line one", "line two"]),
    );
    unmount();
  });

  it("prepends a marker line when the log history is truncated", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => {
      // One line over the 10k cap → truncate to the last 8k plus a marker.
      for (let i = 0; i < 10001; i++) emitLog(`line ${i}`);
    });

    await waitFor(() => expect(result.current.logs).toHaveLength(8001));
    const logs = result.current.logs;
    expect(logs[0]).toContain("earlier log lines dropped");
    expect(logs[logs.length - 1]).toBe("line 10000");
    unmount();
  });

  it("unregisters listeners on unmount", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(3));
  });
});
