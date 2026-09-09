import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { usePipeline } from "./usePipeline";

// Capture registered event handlers so tests can emit synthetic events.
const handlers: Record<string, (event: { payload: unknown }) => void> = {};
const unlisten = vi.fn();
const listenMock = vi.hoisted(() => vi.fn());
const invokeMock = vi.hoisted(() => vi.fn(() => new Promise(() => {})));

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

function emitLog(
  line: string,
  extra?: {
    session?: number;
    label?: string;
    level?: string;
    request?: Record<string, unknown>;
  },
) {
  handlers["pipeline:log"]({ payload: { line, ...extra } });
}

function emitUsage(
  session: number | null,
  input: number,
  output: number,
  cached = 0,
  cacheWrite = 0,
  modelRoundTrips = 0,
  toolCalls: Record<string, number> = {},
) {
  handlers["pipeline:usage"]({
    payload: {
      session,
      input_tokens: input,
      output_tokens: output,
      cached_input_tokens: cached,
      cache_write_input_tokens: cacheWrite,
      model_round_trips: modelRoundTrips,
      tool_calls: toolCalls,
    },
  });
}

function emitStage(stage: string, id = stage, skipped = false) {
  handlers["pipeline:stage"]({
    payload: {
      stage,
      id,
      label: id,
      stepIds: [],
      skipped,
    },
  });
}

function emitPass(name: string, status: string) {
  handlers["pipeline:pass"]({ payload: { name, status } });
}

function emitRouting() {
  handlers["pipeline:routing"]({
    payload: {
      primaryDomain: "Economics",
      subject: "Quantitative macroeconomics",
      subjectIds: ["subject_economics_macro"],
      subjectLabels: ["Economics — Macroeconomics"],
      methodIds: ["formal_proofs"],
      methodLabels: ["Method — Formal Proofs"],
      specialistIds: ["subject_economics_macro", "formal_proofs"],
      specialistLabels: [
        "Economics — Macroeconomics",
        "Method — Formal Proofs",
      ],
    },
  });
}

describe("usePipeline log buffering", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenMock.mockImplementation(
      (name: string, cb: (event: { payload: unknown }) => void) => {
        handlers[name] = cb;
        return Promise.resolve(unlisten);
      },
    );
  });

  it("flushes buffered log lines into state", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => {
      emitLog("line one");
      emitLog("line two", {
        session: 3,
        label: "Orientation map",
        level: "info",
      });
    });

    // Buffered logs are flushed on a short interval rather than per event.
    await waitFor(() => expect(result.current.logs).toHaveLength(2));
    const logs = result.current.logs;
    // Line with no session metadata defaults to a null/master entry.
    // (`t` is a client arrival timestamp — present but not asserted exactly.)
    expect(logs[0]).toEqual({
      line: "line one",
      session: null,
      label: null,
      level: "info",
      t: expect.any(Number),
    });
    // Structured metadata is carried through verbatim.
    expect(logs[1]).toEqual({
      line: "line two",
      session: 3,
      label: "Orientation map",
      level: "info",
      t: expect.any(Number),
    });
    unmount();
  });

  it("prepends a marker line when the log history is truncated", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => {
      emitLog("request summary", {
        session: 9,
        label: "Long-running step",
        request: { prompt: "important prompt" },
      });
      // Cross the 10k cap → retain 8k entries plus a marker.
      for (let i = 0; i < 10001; i++) emitLog(`line ${i}`);
    });

    await waitFor(() => expect(result.current.logs).toHaveLength(8001));
    const logs = result.current.logs;
    expect(logs[0].line).toContain("earlier log lines dropped");
    expect(logs.some((entry) => entry.session === 9 && entry.request)).toBe(
      true,
    );
    expect(logs[logs.length - 1].line).toBe("line 10000");
    unmount();
  });

  it("accumulates token usage per session and in total", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => {
      emitUsage(1, 100, 20, 70, 20, 3, { text_file: 2, web: 1 });
      emitUsage(1, 50, 10, 30, 0, 4, { image: 1, unknown: 2 });
      emitUsage(2, 200, 40, 0, 0, 2, { shell_or_other: 3 });
      emitUsage(null, 5, 5); // orchestration-level: counts toward the total only
    });

    await waitFor(() => expect(result.current.usage.total.input).toBe(355));
    expect(result.current.usage.total.output).toBe(75);
    expect(result.current.usage.total.cached).toBe(100);
    expect(result.current.usage.total.cacheWrite).toBe(20);
    expect(result.current.usage.total.modelRoundTrips).toBe(9);
    expect(result.current.usage.total.toolCalls).toEqual({
      text_file: 2,
      image: 1,
      web: 1,
      shell_or_other: 3,
      unknown: 2,
    });
    expect(result.current.usage.bySession[1]).toEqual({
      input: 150,
      output: 30,
      cached: 100,
      cacheWrite: 20,
      modelRoundTrips: 7,
      toolCalls: {
        text_file: 2,
        image: 1,
        web: 1,
        shell_or_other: 0,
        unknown: 2,
      },
    });
    expect(result.current.usage.bySession[2]).toEqual({
      input: 200,
      output: 40,
      cached: 0,
      cacheWrite: 0,
      modelRoundTrips: 2,
      toolCalls: {
        text_file: 0,
        image: 0,
        web: 0,
        shell_or_other: 3,
        unknown: 0,
      },
    });
    expect(result.current.usage.bySession[3]).toBeUndefined();
    unmount();
  });

  it("ignores queued progress stages after the run reaches done", async () => {
    invokeMock.mockResolvedValueOnce({
      markdown: "# Done",
      extracted_text: "Document",
      run_id: "run-1",
      report: {
        orientation: null,
        step_outputs: [],
        failed_steps: [],
        report_date: "2026-08-15",
        paper_hash: "abcdef1234567890",
      },
    });
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    await act(async () => {
      await result.current.startPipeline("/papers/example.pdf");
    });
    expect(result.current.state.kind).toBe("done");
    expect(result.current.stageHistory).toEqual([]);

    act(() => emitStage("dispatching", "late-dispatch"));
    expect(result.current.state.kind).toBe("done");
    expect(result.current.stageHistory).toEqual([]);
    unmount();
  });

  it("unregisters listeners on unmount", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(6));
  });

  it("cleans partial registrations and fails closed when one listener rejects", async () => {
    const cleanups = [vi.fn(), vi.fn(), vi.fn(), vi.fn(), vi.fn(), vi.fn()];
    let registration = 0;
    listenMock.mockImplementation(
      (name: string, cb: (event: { payload: unknown }) => void) => {
        handlers[name] = cb;
        const index = registration++;
        return index === 1
          ? Promise.reject(new Error("event permission denied"))
          : Promise.resolve(cleanups[index]);
      },
    );
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});
    const { result, unmount } = renderHook(() => usePipeline());

    await waitFor(() =>
      expect(result.current.state).toMatchObject({
        kind: "error",
        message: expect.stringContaining("event permission denied"),
      }),
    );
    expect(result.current.listenersReady).toBe(false);
    expect(cleanups[0]).toHaveBeenCalledTimes(1);
    expect(cleanups[2]).toHaveBeenCalledTimes(1);
    expect(cleanups[3]).toHaveBeenCalledTimes(1);
    expect(cleanups[4]).toHaveBeenCalledTimes(1);
    expect(cleanups[5]).toHaveBeenCalledTimes(1);

    unmount();
    consoleError.mockRestore();
  });

  it("exposes the detected review routing summary", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => emitRouting());
    expect(result.current.reviewRouting).toEqual({
      primaryDomain: "Economics",
      subject: "Quantitative macroeconomics",
      subjectIds: ["subject_economics_macro"],
      subjectLabels: ["Economics — Macroeconomics"],
      methodIds: ["formal_proofs"],
      methodLabels: ["Method — Formal Proofs"],
      specialistIds: ["subject_economics_macro", "formal_proofs"],
      specialistLabels: [
        "Economics — Macroeconomics",
        "Method — Formal Proofs",
      ],
    });
    unmount();
  });

  it("surfaces and updates provider usage-limit recovery notices", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => {
      handlers["pipeline:provider-limit"]({
        payload: {
          pass_key: "technical/claude",
          label: "Step: Technical",
          provider: "claude",
          message: "You've hit your limit · resets 3am",
          status: "fallback_starting",
          fallback_provider: "codex",
          fallback_model: null,
        },
      });
    });
    expect(result.current.providerLimitNotices).toHaveLength(1);
    expect(result.current.providerLimitNotices[0].status).toBe(
      "fallback_starting",
    );

    act(() => {
      handlers["pipeline:provider-limit"]({
        payload: {
          ...result.current.providerLimitNotices[0],
          status: "recovered",
          fallback_model: "gpt-fallback",
        },
      });
    });
    expect(result.current.providerLimitNotices).toEqual([
      expect.objectContaining({
        pass_key: "technical/claude",
        status: "recovered",
        fallback_model: "gpt-fallback",
      }),
    ]);
    unmount();
  });

  it("preserves repeated runtime stages and their pass results", async () => {
    const { result, unmount } = renderHook(() => usePipeline());
    await waitFor(() => expect(result.current.listenersReady).toBe(true));

    act(() => {
      emitStage("dispatching", "wave-0-parallel");
      emitPass("first", "running");
      emitPass("first", "done");
      emitStage("synthesizing", "wave-1-sequential");
      emitPass("middle", "done");
      emitStage("dispatching", "wave-2-parallel");
      emitPass("last", "running");
    });

    expect(result.current.stageHistory.map((stage) => stage.kind)).toEqual([
      "dispatching",
      "synthesizing",
      "dispatching",
    ]);
    expect(result.current.stageHistory[0]).toMatchObject({
      id: "wave-0-parallel",
      status: "done",
      passes: { first: "done" },
    });
    expect(result.current.stageHistory[1]).toMatchObject({
      id: "wave-1-sequential",
      status: "done",
      passes: { middle: "done" },
    });
    expect(result.current.stageHistory[2]).toMatchObject({
      id: "wave-2-parallel",
      status: "active",
      passes: { last: "running" },
    });
    unmount();
  });
});
