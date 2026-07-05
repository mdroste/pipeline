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

function emitLog(line: string, extra?: { session?: number; label?: string; level?: string }) {
  handlers["pipeline:log"]({ payload: { line, ...extra } });
}

function emitUsage(session: number | null, input: number, output: number) {
  handlers["pipeline:usage"]({ payload: { session, input_tokens: input, output_tokens: output } });
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
      emitLog("line two", { session: 3, label: "Orientation map", level: "info" });
    });

    // The flush interval runs every 100ms.
    await waitFor(() => expect(result.current.logs).toHaveLength(2));
    const logs = result.current.logs;
    // Line with no session metadata defaults to a null/master entry.
    expect(logs[0]).toEqual({ line: "line one", session: null, label: null, level: "info" });
    // Structured metadata is carried through verbatim.
    expect(logs[1]).toEqual({
      line: "line two",
      session: 3,
      label: "Orientation map",
      level: "info",
    });
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
    expect(logs[0].line).toContain("earlier log lines dropped");
    expect(logs[logs.length - 1].line).toBe("line 10000");
    unmount();
  });

  it("accumulates token usage per session and in total", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => {
      emitUsage(1, 100, 20);
      emitUsage(1, 50, 10);
      emitUsage(2, 200, 40);
      emitUsage(null, 5, 5); // orchestration-level: counts toward the total only
    });

    await waitFor(() => expect(result.current.usage.total.input).toBe(355));
    expect(result.current.usage.total.output).toBe(75);
    expect(result.current.usage.bySession[1]).toEqual({ input: 150, output: 30 });
    expect(result.current.usage.bySession[2]).toEqual({ input: 200, output: 40 });
    expect(result.current.usage.bySession[3]).toBeUndefined();
    unmount();
  });

  it("unregisters listeners on unmount", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(4));
  });
});
