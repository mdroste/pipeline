import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { PipelineReport, PipelineResult } from "../lib/types";

export type PassStatus = "pending" | "running" | "done" | "error";

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
  const [logs, setLogs] = useState<string[]>([]);
  const [listenersReady, setListenersReady] = useState(false);

  // Buffer incoming log lines in a ref to avoid O(n) array copies per event.
  // A periodic timer flushes the buffer into state in a single update.
  const logBuffer = useRef<string[]>([]);
  const flushTimer = useRef<ReturnType<typeof setInterval> | null>(null);

  // Listen to Tauri events
  useEffect(() => {
    let mounted = true;

    async function setup(): Promise<UnlistenFn[]> {
      // Register all listeners concurrently so there's no window where
      // some events are captured and others aren't.
      const [u1, u2, u3] = await Promise.all([
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
          setState((prev) => {
            if (prev.kind !== "dispatching" && prev.kind !== "merging" && prev.kind !== "synthesizing") return prev;
            return {
              ...prev,
              passes: { ...prev.passes, [name]: status as PassStatus },
            };
          });
        }),
        listen<{ line: string }>("pipeline:log", (event) => {
          if (!mounted) return;
          logBuffer.current.push(event.payload.line);
        }),
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
                `… earlier log lines dropped (showing last ${LOG_KEEP}) …`,
                ...next.slice(-LOG_KEEP),
              ];
            }
            return next;
          });
        }, LOG_FLUSH_INTERVAL);
      }

      if (mounted) setListenersReady(true);
      return [u1, u2, u3];
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
    async (paperPath: string, diff?: boolean) => {
      setState({ kind: "extracting" });
      setLogs([]);
      try {
        const result = await invoke<PipelineResult>("run_pipeline", {
          paperPath,
          diff: diff ?? false,
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

  return { state, logs, startPipeline, cancel, reset, listenersReady };
}
