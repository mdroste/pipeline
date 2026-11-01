import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { usePipeline } from "./usePipeline";
import { router } from "../lib/router";
import type { QueuedRun } from "../lib/types";
import { queuedRun } from "../test/queuedRunFixture";

// Capture registered event handlers so tests can emit synthetic events.
const handlers: Record<string, (event: { payload: unknown }) => void> = {};
const unlisten = vi.fn();
const listenMock = vi.hoisted(() => vi.fn());
const invokeMock = vi.hoisted(() =>
  vi.fn(
    (_command: string, _args?: unknown): Promise<unknown> =>
      new Promise(() => {}),
  ),
);

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

const RUN = "run-a";

function showPage(page: "main" | "home") {
  vi.spyOn(router, "getSnapshot").mockReturnValue({
    route: { page },
    revision: 0,
  });
}

function emitLog(
  line: string,
  extra?: {
    session?: number;
    label?: string;
    level?: string;
    request?: Record<string, unknown>;
  },
  runKey: string | null = RUN,
) {
  handlers["pipeline:log"]({ payload: { line, ...extra, runKey } });
}

function emitUsage(
  session: number | null,
  input: number,
  output: number,
  cached = 0,
  cacheWrite = 0,
  modelRoundTrips = 0,
  toolCalls: Record<string, number> = {},
  runKey = RUN,
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
      runKey,
    },
  });
}

function emitStage(stage: string, id = stage, skipped = false, runKey = RUN) {
  handlers["pipeline:stage"]({
    payload: { stage, id, label: id, stepIds: [], skipped, runKey },
  });
}

function emitPass(name: string, status: string, runKey = RUN) {
  handlers["pipeline:pass"]({ payload: { name, status, runKey } });
}

const queued = queuedRun;

function emitQueue(runs: QueuedRun[], limit = 2) {
  handlers["runs:queue"]({ payload: { limit, runs } });
}

const routing = {
  primaryDomain: "Economics",
  subject: "Quantitative macroeconomics",
  subjectIds: ["subject_economics_macro"],
  subjectLabels: ["Economics — Macroeconomics"],
  methodIds: ["formal_proofs"],
  methodLabels: ["Method — Formal Proofs"],
  specialistIds: ["subject_economics_macro", "formal_proofs"],
  specialistLabels: ["Economics — Macroeconomics", "Method — Formal Proofs"],
};

async function renderReady() {
  const hook = renderHook(() => usePipeline());
  await waitFor(() => expect(hook.result.current.listenersReady).toBe(true));
  return hook;
}

describe("usePipeline run queue", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockImplementation(() => new Promise(() => {}));
    listenMock.mockImplementation(
      (name: string, cb: (event: { payload: unknown }) => void) => {
        handlers[name] = cb;
        return Promise.resolve(unlisten);
      },
    );
  });

  it("flushes buffered log lines into the tagged run only", async () => {
    showPage("main");
    const { result, unmount } = await renderReady();

    act(() => {
      emitLog("line one");
      emitLog("line two", {
        session: 3,
        label: "Orientation map",
        level: "info",
      });
      emitLog("other run", {}, "run-b");
      emitLog("untagged", {}, null);
    });

    // Buffered logs are flushed on a short interval rather than per event.
    await waitFor(() =>
      expect(result.current.views[RUN]?.logs).toHaveLength(2),
    );
    const logs = result.current.views[RUN].logs;
    // (`t` is a client arrival timestamp — present but not asserted exactly.)
    expect(logs[0]).toEqual({
      line: "line one",
      session: null,
      label: null,
      level: "info",
      t: expect.any(Number),
    });
    expect(logs[1]).toEqual({
      line: "line two",
      session: 3,
      label: "Orientation map",
      level: "info",
      t: expect.any(Number),
    });
    expect(
      result.current.views["run-b"].logs.map((entry) => entry.line),
    ).toEqual(["other run"]);
    expect(Object.keys(result.current.views).sort()).toEqual([RUN, "run-b"]);
    unmount();
  });

  it("holds log lines longer while no page is showing them", async () => {
    showPage("home");
    const { result, unmount } = await renderReady();
    vi.useFakeTimers();
    try {
      act(() => emitLog("line one"));
      act(() => {
        vi.advanceTimersByTime(1000);
      });
      expect(result.current.views[RUN]?.logs ?? []).toHaveLength(0);
      act(() => {
        vi.advanceTimersByTime(1000);
      });
      expect(result.current.views[RUN]?.logs).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
    unmount();
  });

  it("prepends a marker line when a run's log history is truncated", async () => {
    showPage("main");
    const { result, unmount } = await renderReady();

    act(() => {
      emitLog("request summary", {
        session: 9,
        label: "Long-running step",
        request: { prompt: "important prompt" },
      });
      // Cross the 10k cap → retain 8k entries plus a marker.
      for (let i = 0; i < 10001; i++) emitLog(`line ${i}`);
    });

    await waitFor(() =>
      expect(result.current.views[RUN]?.logs).toHaveLength(8001),
    );
    const logs = result.current.views[RUN].logs;
    expect(logs[0].line).toContain("earlier log lines dropped");
    expect(logs.some((entry) => entry.session === 9 && entry.request)).toBe(
      true,
    );
    expect(logs[logs.length - 1].line).toBe("line 10000");
    unmount();
  });

  it("accumulates token usage per run, per session, and in total", async () => {
    const { result, unmount } = await renderReady();

    act(() => {
      emitUsage(1, 100, 20, 70, 20, 3, { text_file: 2, web: 1 });
      emitUsage(1, 50, 10, 30, 0, 4, { image: 1, unknown: 2 });
      emitUsage(2, 200, 40, 0, 0, 2, { shell_or_other: 3 });
      emitUsage(null, 5, 5); // orchestration-level: counts toward the total only
      emitUsage(1, 999, 999, 0, 0, 0, {}, "run-b");
    });

    const usage = result.current.views[RUN].usage;
    expect(usage.total.input).toBe(355);
    expect(usage.total.output).toBe(75);
    expect(usage.total.cached).toBe(100);
    expect(usage.total.cacheWrite).toBe(20);
    expect(usage.total.modelRoundTrips).toBe(9);
    expect(usage.total.toolCalls).toEqual({
      text_file: 2,
      image: 1,
      web: 1,
      shell_or_other: 3,
      unknown: 2,
    });
    expect(usage.bySession[1]).toEqual({
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
    expect(usage.bySession[2].input).toBe(200);
    expect(usage.bySession[3]).toBeUndefined();
    expect(result.current.views["run-b"].usage.total.input).toBe(999);
    unmount();
  });

  it("follows the queue, loads finished results, and ignores late stages", async () => {
    invokeMock.mockImplementation((command: string) =>
      command === "get_queued_run_result"
        ? Promise.resolve({
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
          })
        : new Promise(() => {}),
    );
    const { result, unmount } = await renderReady();

    // A run first seen through the queue (e.g. after a reload) shows its
    // current stage even though its earlier events are gone.
    act(() =>
      emitQueue([
        queued(RUN, "running", { stage: "dispatching" }),
        queued("run-b", "queued"),
      ]),
    );
    expect(result.current.queue.limit).toBe(2);
    expect(result.current.views[RUN].state).toEqual({
      kind: "dispatching",
      passes: {},
    });
    expect(result.current.views[RUN].runStartedAt).toBe(1000);
    expect(result.current.views["run-b"].state.kind).toBe("idle");

    act(() =>
      emitQueue([
        queued(RUN, "done", { resultAvailable: true, runId: "run-1" }),
        queued("run-b", "running"),
      ]),
    );
    await waitFor(() =>
      expect(result.current.views[RUN].state.kind).toBe("done"),
    );
    expect(invokeMock).toHaveBeenCalledWith("get_queued_run_result", {
      key: RUN,
    });
    expect(result.current.views["run-b"].state.kind).toBe("extracting");

    act(() => emitStage("dispatching", "late-dispatch"));
    expect(result.current.views[RUN].state.kind).toBe("done");
    expect(result.current.views[RUN].stageHistory).toEqual([]);

    // Dismissed runs drop their views.
    act(() => emitQueue([queued("run-b", "running")]));
    expect(result.current.views[RUN]).toBeUndefined();
    unmount();
  });

  it("turns failed and cancelled queue entries into error states", async () => {
    const { result, unmount } = await renderReady();

    act(() => {
      emitQueue([queued(RUN, "running"), queued("run-b", "queued")]);
      emitStage("dispatching", "wave-0");
    });
    act(() =>
      emitQueue([
        queued(RUN, "failed", { error: "provider exploded" }),
        queued("run-b", "cancelled", { startedAt: null }),
      ]),
    );
    expect(result.current.views[RUN].state).toEqual({
      kind: "error",
      message: "provider exploded",
      failedAt: "dispatching",
    });
    expect(result.current.views[RUN].stageHistory[0].status).toBe("failed");
    expect(result.current.views["run-b"].state).toMatchObject({
      kind: "error",
      message: "Cancelled before it started.",
    });
    unmount();
  });

  it("queues a review and adopts it before the queue event arrives", async () => {
    invokeMock.mockImplementation((command: string) =>
      command === "enqueue_run"
        ? Promise.resolve(queued("run-new", "queued"))
        : command === "cancel_pipeline"
          ? Promise.resolve(undefined)
          : new Promise(() => {}),
    );
    const { result, unmount } = await renderReady();

    let key = "";
    await act(async () => {
      key = (
        await result.current.enqueue({
          paperPath: "/papers/new.pdf",
          inputInterpretation: "document",
          diff: false,
          variables: null,
          extraInputs: null,
          expectedProfileSnapshotId: "snapshot-1",
        })
      ).key;
    });
    expect(invokeMock).toHaveBeenCalledWith("enqueue_run", {
      paperPath: "/papers/new.pdf",
      inputInterpretation: "document",
      diff: false,
      variables: null,
      extraInputs: null,
      expectedProfileSnapshotId: "snapshot-1",
    });
    expect(key).toBe("run-new");
    expect(result.current.queue.runs.map((run) => run.key)).toEqual([
      "run-new",
    ]);
    expect(result.current.views["run-new"].state.kind).toBe("idle");

    await act(() => result.current.cancel("run-new"));
    expect(invokeMock).toHaveBeenCalledWith("cancel_pipeline", {
      runKey: "run-new",
    });
    unmount();
  });

  it("unregisters listeners on unmount", async () => {
    const { unmount } = await renderReady();

    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(7));
  });

  it("cleans partial registrations and fails closed when one listener rejects", async () => {
    const cleanups = Array.from({ length: 7 }, () => vi.fn());
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
      expect(result.current.setupError).toContain("event permission denied"),
    );
    expect(result.current.listenersReady).toBe(false);
    for (const [index, cleanup] of cleanups.entries())
      if (index !== 1) expect(cleanup).toHaveBeenCalledTimes(1);

    unmount();
    consoleError.mockRestore();
  });

  it("exposes each run's detected review routing summary", async () => {
    const { result, unmount } = await renderReady();

    act(() =>
      handlers["pipeline:routing"]({ payload: { ...routing, runKey: RUN } }),
    );
    expect(result.current.views[RUN].reviewRouting).toEqual(routing);
    unmount();
  });

  it("surfaces and updates provider usage-limit recovery notices per run", async () => {
    const { result, unmount } = await renderReady();
    const notice = {
      pass_key: "technical/claude",
      label: "Step: Technical",
      provider: "claude",
      message: "You've hit your limit · resets 3am",
      status: "fallback_starting",
      fallback_provider: "codex",
      fallback_model: null,
    };

    act(() =>
      handlers["pipeline:provider-limit"]({
        payload: { ...notice, runKey: RUN },
      }),
    );
    expect(result.current.views[RUN].providerLimitNotices).toEqual([notice]);

    act(() =>
      handlers["pipeline:provider-limit"]({
        payload: {
          ...notice,
          status: "recovered",
          fallback_model: "gpt-fallback",
          runKey: RUN,
        },
      }),
    );
    expect(result.current.views[RUN].providerLimitNotices).toEqual([
      expect.objectContaining({
        pass_key: "technical/claude",
        status: "recovered",
        fallback_model: "gpt-fallback",
      }),
    ]);
    unmount();
  });

  it("preserves repeated runtime stages and their pass results", async () => {
    const { result, unmount } = await renderReady();

    act(() => {
      emitStage("dispatching", "wave-0-parallel");
      emitPass("first", "running");
      emitPass("first", "done");
      emitStage("synthesizing", "wave-1-sequential");
      emitPass("middle", "done");
      emitStage("dispatching", "wave-2-parallel");
      emitPass("last", "running");
    });

    const stages = result.current.views[RUN].stageHistory;
    expect(stages.map((stage) => stage.kind)).toEqual([
      "dispatching",
      "synthesizing",
      "dispatching",
    ]);
    expect(stages[0]).toMatchObject({
      id: "wave-0-parallel",
      status: "done",
      passes: { first: "done" },
    });
    expect(stages[1]).toMatchObject({
      id: "wave-1-sequential",
      status: "done",
      passes: { middle: "done" },
    });
    expect(stages[2]).toMatchObject({
      id: "wave-2-parallel",
      status: "active",
      passes: { last: "running" },
    });
    unmount();
  });
});
