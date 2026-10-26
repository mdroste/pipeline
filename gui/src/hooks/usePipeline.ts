import { useState, useEffect, useCallback, useRef } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  PipelineReport,
  QueuedRun,
  RunQueueSnapshot,
  ToolCallCounts,
} from "../lib/types";
import {
  appendLogs,
  applyPass,
  applyProviderLimit,
  applyStage,
  applyUsage,
  completeRun,
  emptyRunView,
  syncWithQueue,
  type RunView,
  type StagePayload,
  type UsagePayload,
} from "../lib/runViews";
import {
  appClient,
  type EnqueueRunRequest,
  type StartBatchRequest,
} from "../lib/appClient";

export type PassStatus = "pending" | "running" | "done" | "error" | "skipped";

/** Effective settings and prompt material for one provider invocation. Secrets
 *  are never included. Large prompt fields arrive only in the live event and
 *  are not written to the persistent console transcript. */
export interface LlmRequestDetails {
  provider: string;
  provider_label: string;
  transport: "cli" | "api";
  model: string;
  model_policy: string;
  effort: string;
  tools: string[];
  timeout_secs: number;
  max_output_tokens: number | null;
  output_format: string;
  prompt: string;
  prompt_chars: number;
  prompt_truncated?: boolean;
  system_prompt: string | null;
  system_prompt_chars?: number;
  system_prompt_truncated?: boolean;
  shared_context: string | null;
  shared_context_chars: number;
  shared_context_truncated?: boolean;
  pdf_attached: boolean;
  write_enabled: boolean;
  working_directory: string | null;
  read_directories: string[];
  write_directory: string | null;
  local_endpoint: string | null;
}

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
  /** Present on the first line for an LLM session. */
  request?: LlmRequestDetails;
}

/** Start/end times (ms epoch) of a single pass, for elapsed-time display. */
export interface PassTiming {
  start: number;
  end?: number;
}

export type RuntimeStageKind =
  | "extracting"
  | "orienting"
  | "dispatching"
  | "merging"
  | "synthesizing"
  | "done";

export interface RuntimeStage {
  /** Stable ID emitted by the backend and shared with get_execution_plan. */
  id: string;
  kind: RuntimeStageKind;
  label: string;
  stepIds: string[];
  stepLabels?: string[];
  /** Multi-provider steps that will receive a merge after this dispatch wave. */
  mergeStepIds?: string[];
  mergeStepLabels?: string[];
  passes: Record<string, PassStatus>;
  status: "active" | "done" | "failed" | "skipped";
}

export interface ReviewRoutingSummary {
  primaryDomain: string;
  subject: string;
  subjectIds?: string[];
  subjectLabels?: string[];
  methodIds?: string[];
  methodLabels?: string[];
  specialistIds: string[];
  specialistLabels: string[];
}

export interface ProviderLimitNotice {
  pass_key: string;
  label: string;
  provider: string;
  message: string;
  status: "fallback_starting" | "recovered" | "fallback_failed" | "exhausted";
  fallback_provider: string | null;
  fallback_model: string | null;
}

export interface TokenTotals {
  input: number;
  output: number;
  cached: number;
  cacheWrite: number;
  modelRoundTrips: number;
  toolCalls: ToolCallCounts;
}

/** Token usage aggregated over a run: a grand total plus per-session counts.
 *  Every current provider reports usage — Codex, Claude CLI JSON,
 *  and the direct APIs. */
export interface UsageState {
  total: TokenTotals;
  bySession: Record<number, TokenTotals>;
}

export type PipelineState =
  | { kind: "idle" }
  | { kind: "extracting" }
  | { kind: "orienting" }
  | { kind: "dispatching"; passes: Record<string, PassStatus> }
  | { kind: "merging"; passes: Record<string, PassStatus> }
  | { kind: "synthesizing"; passes: Record<string, PassStatus> }
  | {
      kind: "done";
      markdown: string;
      report: PipelineReport;
      extractedText: string;
      runId: string | null;
    }
  | { kind: "error"; message: string; failedAt?: string };

/** Interval (ms) at which buffered log lines are flushed to state. */
const LOG_FLUSH_INTERVAL = 200;

export type { RunView };

/** Events from one run carry its queue key; untagged events are ignored. */
type Tagged<T> = T & { runKey?: string };

/** Follow every review in the run queue: its queue entry plus a live view
 *  (progress, console, usage, result) built from its tagged events. */
export function usePipeline() {
  const [queue, setQueue] = useState<RunQueueSnapshot>({ limit: 1, runs: [] });
  const [views, setViews] = useState<Record<string, RunView>>({});
  const [listenersReady, setListenersReady] = useState(false);
  const [setupError, setSetupError] = useState<string | null>(null);
  // Buffer incoming log lines per run to avoid O(n) array copies per event.
  // A periodic timer flushes every buffer into state in a single update.
  const logBuffer = useRef(new Map<string, LogEntry[]>());
  const flushTimer = useRef<ReturnType<typeof setInterval> | null>(null);
  const requestedResults = useRef(new Set<string>());

  const updateView = useCallback(
    (key: string | undefined, update: (view: RunView) => RunView) => {
      if (!key) return;
      setViews((previous) => ({
        ...previous,
        [key]: update(previous[key] ?? emptyRunView()),
      }));
    },
    [],
  );

  const applyQueue = useCallback((snapshot: RunQueueSnapshot) => {
    setQueue(snapshot);
    // The queue is authoritative: views of dismissed or pruned runs go.
    setViews((previous) => {
      const next: Record<string, RunView> = {};
      for (const run of snapshot.runs)
        next[run.key] = syncWithQueue(previous[run.key], run);
      return next;
    });
    const live = new Set(snapshot.runs.map((run) => run.key));
    for (const key of logBuffer.current.keys())
      if (!live.has(key)) logBuffer.current.delete(key);
  }, []);

  useEffect(() => {
    let mounted = true;

    async function setup(): Promise<UnlistenFn[]> {
      // Register all listeners concurrently so there's no window where
      // some events are captured and others aren't.
      const registrations = await Promise.allSettled([
        listen<Tagged<StagePayload>>("pipeline:stage", (event) => {
          if (!mounted) return;
          updateView(event.payload.runKey, (view) =>
            applyStage(view, event.payload),
          );
        }),
        listen<Tagged<{ name: string; status: string }>>(
          "pipeline:pass",
          (event) => {
            if (!mounted) return;
            const now = Date.now();
            updateView(event.payload.runKey, (view) =>
              applyPass(view, event.payload, now),
            );
          },
        ),
        listen<Tagged<ReviewRoutingSummary>>("pipeline:routing", (event) => {
          if (!mounted) return;
          const { runKey, ...routing } = event.payload;
          updateView(runKey, (view) => ({ ...view, reviewRouting: routing }));
        }),
        listen<Tagged<ProviderLimitNotice>>(
          "pipeline:provider-limit",
          (event) => {
            if (!mounted) return;
            const { runKey, ...notice } = event.payload;
            updateView(runKey, (view) => applyProviderLimit(view, notice));
          },
        ),
        listen<
          Tagged<{
            line: string;
            session?: number | null;
            label?: string | null;
            level?: string;
            request?: LlmRequestDetails;
          }>
        >("pipeline:log", (event) => {
          if (!mounted || !event.payload.runKey) return;
          const pending = logBuffer.current.get(event.payload.runKey) ?? [];
          pending.push({
            line: event.payload.line,
            session: event.payload.session ?? null,
            label: event.payload.label ?? null,
            level: event.payload.level ?? "info",
            t: Date.now(),
            ...(event.payload.request
              ? { request: event.payload.request }
              : {}),
          });
          logBuffer.current.set(event.payload.runKey, pending);
        }),
        // Usage events are infrequent (one per LLM call), so update state
        // directly rather than through the log buffer.
        listen<Tagged<UsagePayload>>("pipeline:usage", (event) => {
          if (!mounted) return;
          updateView(event.payload.runKey, (view) =>
            applyUsage(view, event.payload),
          );
        }),
        listen<RunQueueSnapshot>("runs:queue", (event) => {
          if (mounted) applyQueue(event.payload);
        }),
      ]);
      const unlisteners = registrations.flatMap((registration) =>
        registration.status === "fulfilled" ? [registration.value] : [],
      );
      const failed = registrations.find(
        (registration): registration is PromiseRejectedResult =>
          registration.status === "rejected",
      );
      if (failed) {
        // Promise.all would discard successful registrations when one listener
        // fails. Tear those down before surfacing the fatal setup error.
        unlisteners.forEach((unlisten) => unlisten());
        throw failed.reason;
      }

      // Periodically flush buffered log lines into React state. Don't start
      // the timer if the component unmounted while listener registration was
      // in flight — cleanup has already run and nothing would clear it.
      if (mounted) {
        flushTimer.current = setInterval(() => {
          if (!mounted || logBuffer.current.size === 0) return;
          const pending = logBuffer.current;
          logBuffer.current = new Map();
          setViews((previous) => {
            const next = { ...previous };
            for (const [key, lines] of pending)
              next[key] = {
                ...(next[key] ?? emptyRunView()),
                logs: appendLogs(next[key]?.logs ?? [], lines),
              };
            return next;
          });
        }, LOG_FLUSH_INTERVAL);
      }

      if (mounted) setListenersReady(true);
      // Listeners first, then the snapshot, so a run that starts in between
      // is never missed. Reviews survive a window reload; their progress
      // resumes from the queue entry.
      appClient
        .runQueue()
        .then((snapshot) => {
          if (mounted) applyQueue(snapshot);
        })
        .catch((error: unknown) => {
          console.warn("Unable to load the review queue:", error);
        });
      return unlisteners;
    }

    const setupPromise = setup();
    // If listener registration fails, the start button (gated on
    // listenersReady) would otherwise stay disabled forever with no feedback.
    setupPromise.catch((e: unknown) => {
      if (!mounted) return;
      const message = e instanceof Error ? e.message : String(e);
      console.error("Failed to register Tauri event listeners:", e);
      setSetupError(
        `Failed to register event listeners: ${message}. Reload the window to retry.`,
      );
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
  }, [applyQueue, updateView]);

  // Finished reviews: load the full result the queue still holds.
  useEffect(() => {
    for (const run of queue.runs) {
      if (
        run.status !== "done" ||
        !run.resultAvailable ||
        requestedResults.current.has(run.key)
      )
        continue;
      requestedResults.current.add(run.key);
      appClient
        .queuedRunResult(run.key)
        .then((result) => {
          if (result) updateView(run.key, (view) => completeRun(view, result));
        })
        .catch((error: unknown) => {
          requestedResults.current.delete(run.key);
          console.warn("Unable to load the finished review:", error);
        });
    }
  }, [queue, updateView]);

  /** Record a just-queued run before its first queue event arrives, so the
   *  caller can open it immediately. */
  const adopt = useCallback((runs: QueuedRun[]) => {
    setQueue((previous) => {
      const known = new Set(previous.runs.map((run) => run.key));
      const added = runs.filter((run) => !known.has(run.key));
      return added.length
        ? { ...previous, runs: [...previous.runs, ...added] }
        : previous;
    });
    setViews((previous) => {
      const next = { ...previous };
      for (const run of runs) next[run.key] = syncWithQueue(next[run.key], run);
      return next;
    });
  }, []);

  const enqueue = useCallback(
    async (request: EnqueueRunRequest) => {
      const run = await appClient.enqueueRun(request);
      adopt([run]);
      return run;
    },
    [adopt],
  );

  const enqueueBatch = useCallback(
    async (request: StartBatchRequest) => {
      const runs = await appClient.enqueueBatch(request);
      adopt(runs);
      return runs;
    },
    [adopt],
  );

  const enqueueRerun = useCallback(
    async (
      runId: string,
      opts?: { fromStep?: string; onlyFailed?: boolean },
    ) => {
      const run = await appClient.enqueueRerun(
        runId,
        opts?.onlyFailed ?? false,
        opts?.fromStep,
      );
      adopt([run]);
      return run;
    },
    [adopt],
  );

  const cancel = useCallback(async (key: string) => {
    await appClient.cancelRun(key);
  }, []);

  const dismiss = useCallback(async (key?: string) => {
    await appClient.dismissRuns(key);
  }, []);

  return {
    queue,
    views,
    listenersReady,
    setupError,
    enqueue,
    enqueueBatch,
    enqueueRerun,
    cancel,
    dismiss,
  };
}
