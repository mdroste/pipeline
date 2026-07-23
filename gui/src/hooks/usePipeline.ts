import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { PipelineReport, PipelineResult } from "../lib/types";

export type PassStatus = "pending" | "running" | "done" | "error" | "skipped";

/** One console line, tagged with the headless session (LLM call) it came from.
 *  `session` is null for orchestration/extraction lines that belong to no
 *  single call; those show only in the master view. */
export interface LogEntry {
  line: string;
  session: number | null;
  label: string | null;
  level: string; // info | warn | error | stderr | stdout
  /** Client arrival time (ms epoch), used for optional console timestamps. */
  t: number;
}

/** Start/end times (ms epoch) of a single pass, for elapsed-time display. */
export interface PassTiming {
  start: number;
  end?: number;
}

export interface TokenTotals {
  input: number;
  output: number;
  cached: number;
  cacheWrite: number;
}

/** Token usage aggregated over a run: a grand total plus per-session counts.
 *  Only providers that report usage (codex CLI, claude CLI JSON, and the
 *  direct APIs) contribute; text-mode gemini CLI reports nothing. */
export interface UsageState {
  total: TokenTotals;
  bySession: Record<number, TokenTotals>;
}

const EMPTY_USAGE: UsageState = {
  total: { input: 0, output: 0, cached: 0, cacheWrite: 0 },
  bySession: {},
};

export type PipelineState =
  | { kind: "idle" }
  | { kind: "extracting" }
  | { kind: "orienting" }
  | { kind: "dispatching"; passes: Record<string, PassStatus> }
  | { kind: "merging"; passes: Record<string, PassStatus> }
  | { kind: "synthesizing"; passes: Record<string, PassStatus> }
  | { kind: "done"; markdown: string; report: PipelineReport; extractedText: string; runId: string | null }
  | { kind: "error"; message: string; failedAt?: string };

/** Interval (ms) at which buffered log lines are flushed to state. */
const LOG_FLUSH_INTERVAL = 100;
/** Maximum log entries to keep. */
const LOG_MAX = 10000;
/** When truncating, keep this many entries. */
const LOG_KEEP = 8000;

export function usePipeline() {
  const [state, setState] = useState<PipelineState>({ kind: "idle" });
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [usage, setUsage] = useState<UsageState>(EMPTY_USAGE);
  const [listenersReady, setListenersReady] = useState(false);
  // Run-elapsed clock and per-pass timings, for the progress view.
  const [runStartedAt, setRunStartedAt] = useState<number | null>(null);
  const [passTimes, setPassTimes] = useState<Record<string, PassTiming>>({});

  // Buffer incoming log lines in a ref to avoid O(n) array copies per event.
  // A periodic timer flushes the buffer into state in a single update.
  const logBuffer = useRef<LogEntry[]>([]);
  const flushTimer = useRef<ReturnType<typeof setInterval> | null>(null);

  // Listen to Tauri events
  useEffect(() => {
    let mounted = true;

    async function setup(): Promise<UnlistenFn[]> {
      // Register all listeners concurrently so there's no window where
      // some events are captured and others aren't.
      const [u1, u2, u3, u4] = await Promise.all([
        listen<{ stage: string }>("pipeline:stage", (event) => {
          if (!mounted) return;
          const { stage } = event.payload;
          switch (stage) {
            case "extracting":
              setState({ kind: "extracting" });
              break;
            case "orienting":
              setState({ kind: "orienting" });
              break;
            case "dispatching":
              setState({
                kind: "dispatching",
                passes: {},
              });
              break;
            case "merging":
              setState({
                kind: "merging",
                passes: {},
              });
              break;
            case "synthesizing":
              setState({ kind: "synthesizing", passes: {} });
              break;
          }
        }),
        listen<{ name: string; status: string }>("pipeline:pass", (event) => {
          if (!mounted) return;
          const { name, status } = event.payload;
          const now = Date.now();
          setPassTimes((prev) => {
            const cur = prev[name];
            if (status === "running") {
              // First "running" marks the start; retries keep the original start.
              if (cur && cur.end === undefined) return prev;
              return { ...prev, [name]: { start: now } };
            }
            // done / error close out the pass.
            return { ...prev, [name]: { start: cur?.start ?? now, end: now } };
          });
          setState((prev) => {
            if (prev.kind !== "dispatching" && prev.kind !== "merging" && prev.kind !== "synthesizing") return prev;
            return {
              ...prev,
              passes: { ...prev.passes, [name]: status as PassStatus },
            };
          });
        }),
        listen<{ line: string; session?: number | null; label?: string | null; level?: string }>(
          "pipeline:log",
          (event) => {
            if (!mounted) return;
            logBuffer.current.push({
              line: event.payload.line,
              session: event.payload.session ?? null,
              label: event.payload.label ?? null,
              level: event.payload.level ?? "info",
              t: Date.now(),
            });
          }
        ),
        listen<{
          session: number | null;
          input_tokens: number;
          output_tokens: number;
          cached_input_tokens?: number;
          cache_write_input_tokens?: number;
        }>(
          "pipeline:usage",
          (event) => {
            if (!mounted) return;
            const {
              session,
              input_tokens,
              output_tokens,
              cached_input_tokens = 0,
              cache_write_input_tokens = 0,
            } = event.payload;
            // Usage events are infrequent (one per LLM call), so update state
            // directly rather than through the log buffer.
            setUsage((prev) => {
              const total = {
                input: prev.total.input + input_tokens,
                output: prev.total.output + output_tokens,
                cached: prev.total.cached + cached_input_tokens,
                cacheWrite: prev.total.cacheWrite + cache_write_input_tokens,
              };
              const bySession = { ...prev.bySession };
              if (session != null) {
                const cur = bySession[session] ?? {
                  input: 0,
                  output: 0,
                  cached: 0,
                  cacheWrite: 0,
                };
                bySession[session] = {
                  input: cur.input + input_tokens,
                  output: cur.output + output_tokens,
                  cached: cur.cached + cached_input_tokens,
                  cacheWrite: cur.cacheWrite + cache_write_input_tokens,
                };
              }
              return { total, bySession };
            });
          }
        ),
      ]);

      // Periodically flush buffered log lines into React state. Don't start
      // the timer if the component unmounted while listener registration was
      // in flight — cleanup has already run and nothing would clear it.
      if (mounted) {
        flushTimer.current = setInterval(() => {
          if (!mounted) return;
          const pending = logBuffer.current;
          if (pending.length === 0) return;
          logBuffer.current = [];
          setLogs((prev) => {
            const next = prev.concat(pending);
            if (next.length > LOG_MAX) {
              return [
                {
                  line: `… earlier log lines dropped (showing last ${LOG_KEEP}) …`,
                  session: null,
                  label: null,
                  level: "info",
                  t: Date.now(),
                },
                ...next.slice(-LOG_KEEP),
              ];
            }
            return next;
          });
        }, LOG_FLUSH_INTERVAL);
      }

      if (mounted) setListenersReady(true);
      return [u1, u2, u3, u4];
    }

    const setupPromise = setup();
    // If listener registration fails, the Generate button (gated on
    // listenersReady) would otherwise stay disabled forever with no feedback.
    // Surface the failure via the error state so the UI can render it.
    setupPromise.catch((e: unknown) => {
      if (!mounted) return;
      const message = e instanceof Error ? e.message : String(e);
      console.error("Failed to register Tauri event listeners:", e);
      setState({
        kind: "error",
        message: `Failed to register event listeners: ${message}. Reload the window to retry.`,
      });
    });

    return () => {
      mounted = false;
      if (flushTimer.current) clearInterval(flushTimer.current);
      setupPromise
        .then((fns) => {
          fns.forEach((fn) => fn());
          // Belt-and-braces: if setup managed to start the timer despite the
          // unmount race, clear it now.
          if (flushTimer.current) clearInterval(flushTimer.current);
        })
        .catch(() => {});
    };
  }, []);

  const startPipeline = useCallback(
    async (
      paperPath: string,
      diff?: boolean,
      variables?: Record<string, string>,
      extraInputs?: Record<string, string>
    ) => {
      setState({ kind: "extracting" });
      setLogs([]);
      setUsage(EMPTY_USAGE);
      setRunStartedAt(Date.now());
      setPassTimes({});
      try {
        const result = await invoke<PipelineResult>("run_pipeline", {
          paperPath,
          diff: diff ?? false,
          variables: variables ?? null,
          extraInputs: extraInputs ?? null,
        });
        setState({
          kind: "done",
          markdown: result.markdown,
          report: result.report,
          extractedText: result.extracted_text,
          runId: result.run_id ?? null,
        });
      } catch (e: unknown) {
        const message = e instanceof Error ? e.message : String(e);
        setState((prev) => ({
          kind: "error" as const,
          message,
          failedAt: prev.kind === "error" ? undefined : prev.kind,
        }));
      }
    },
    []
  );

  const rerunPipeline = useCallback(
    async (runId: string, opts?: { fromStep?: string; onlyFailed?: boolean }) => {
      setState({ kind: "extracting" });
      setLogs([]);
      setUsage(EMPTY_USAGE);
      setRunStartedAt(Date.now());
      setPassTimes({});
      try {
        const result = await invoke<PipelineResult>("rerun_run", {
          runId,
          fromStep: opts?.fromStep ?? null,
          onlyFailed: opts?.onlyFailed ?? false,
        });
        setState({
          kind: "done",
          markdown: result.markdown,
          report: result.report,
          extractedText: result.extracted_text,
          runId: result.run_id ?? null,
        });
      } catch (e: unknown) {
        const message = e instanceof Error ? e.message : String(e);
        setState((prev) => ({
          kind: "error" as const,
          message,
          failedAt: prev.kind === "error" ? undefined : prev.kind,
        }));
      }
    },
    []
  );

  const cancel = useCallback(async () => {
    try {
      await invoke("cancel_pipeline");
    } catch {
      // ignore
    }
  }, []);

  const reset = useCallback(() => {
    setState({ kind: "idle" });
  }, []);

  return { state, logs, usage, startPipeline, rerunPipeline, cancel, reset, listenersReady, runStartedAt, passTimes };
}
