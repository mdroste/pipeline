import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  PipelineReport,
  PipelineResult,
  RunParallelOverrides,
  ToolCallCounts,
} from "../lib/types";

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

const EMPTY_USAGE: UsageState = {
  total: {
    input: 0,
    output: 0,
    cached: 0,
    cacheWrite: 0,
    modelRoundTrips: 0,
    toolCalls: {
      text_file: 0,
      image: 0,
      web: 0,
      shell_or_other: 0,
      unknown: 0,
    },
  },
  bySession: {},
};

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
  const [stageHistory, setStageHistory] = useState<RuntimeStage[]>([]);
  const [reviewRouting, setReviewRouting] =
    useState<ReviewRoutingSummary | null>(null);
  const [providerLimitNotices, setProviderLimitNotices] = useState<
    ProviderLimitNotice[]
  >([]);
  const stageSequence = useRef(0);
  // Tauri events and invoke responses travel over separate channels. Once the
  // command has reached a terminal state, ignore any earlier stage event that
  // was still queued for delivery instead of regressing the finished view.
  const terminalState = useRef(false);

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
      const registrations = await Promise.allSettled([
        listen<{
          stage: string;
          id: string;
          label: string;
          stepIds: string[];
          stepLabels?: string[];
          mergeStepIds?: string[];
          mergeStepLabels?: string[];
          skipped?: boolean;
        }>("pipeline:stage", (event) => {
          if (!mounted || terminalState.current) return;
          const {
            stage,
            id,
            label,
            stepIds,
            stepLabels = [],
            mergeStepIds = [],
            mergeStepLabels = [],
            skipped = false,
          } = event.payload;
          if (
            stage !== "extracting" &&
            stage !== "orienting" &&
            stage !== "dispatching" &&
            stage !== "merging" &&
            stage !== "synthesizing" &&
            stage !== "done"
          )
            return;
          const kind = stage as RuntimeStageKind;
          setStageHistory((previous) => {
            const next = previous.map((entry, index) =>
              index === previous.length - 1 && entry.status === "active"
                ? { ...entry, status: "done" as const }
                : entry,
            );
            next.push({
              id: id || `runtime-${++stageSequence.current}`,
              kind,
              label,
              stepIds,
              stepLabels,
              mergeStepIds,
              mergeStepLabels,
              passes: {},
              status: skipped ? "skipped" : "active",
            });
            return next;
          });
          if (skipped || stage === "done") return;
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
            if (status === "pending") return prev;
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
            if (
              prev.kind !== "dispatching" &&
              prev.kind !== "merging" &&
              prev.kind !== "synthesizing"
            )
              return prev;
            return {
              ...prev,
              passes: { ...prev.passes, [name]: status as PassStatus },
            };
          });
          setStageHistory((previous) =>
            previous.map((entry, index) =>
              index === previous.length - 1 && entry.status === "active"
                ? {
                    ...entry,
                    passes: { ...entry.passes, [name]: status as PassStatus },
                  }
                : entry,
            ),
          );
        }),
        listen<ReviewRoutingSummary>("pipeline:routing", (event) => {
          if (!mounted) return;
          setReviewRouting(event.payload);
        }),
        listen<ProviderLimitNotice>("pipeline:provider-limit", (event) => {
          if (!mounted) return;
          setProviderLimitNotices((previous) => {
            const next = previous.filter(
              (notice) => notice.pass_key !== event.payload.pass_key,
            );
            next.push(event.payload);
            return next.slice(-20);
          });
        }),
        listen<{
          line: string;
          session?: number | null;
          label?: string | null;
          level?: string;
          request?: LlmRequestDetails;
        }>("pipeline:log", (event) => {
          if (!mounted) return;
          logBuffer.current.push({
            line: event.payload.line,
            session: event.payload.session ?? null,
            label: event.payload.label ?? null,
            level: event.payload.level ?? "info",
            t: Date.now(),
            ...(event.payload.request
              ? { request: event.payload.request }
              : {}),
          });
        }),
        listen<{
          session: number | null;
          input_tokens: number;
          output_tokens: number;
          cached_input_tokens?: number;
          cache_write_input_tokens?: number;
          model_round_trips?: number;
          tool_calls?: Partial<ToolCallCounts>;
        }>("pipeline:usage", (event) => {
          if (!mounted) return;
          const {
            session,
            input_tokens,
            output_tokens,
            cached_input_tokens = 0,
            cache_write_input_tokens = 0,
            model_round_trips = 0,
            tool_calls = {},
          } = event.payload;
          // Usage events are infrequent (one per LLM call), so update state
          // directly rather than through the log buffer.
          setUsage((prev) => {
            const total = {
              input: prev.total.input + input_tokens,
              output: prev.total.output + output_tokens,
              cached: prev.total.cached + cached_input_tokens,
              cacheWrite: prev.total.cacheWrite + cache_write_input_tokens,
              modelRoundTrips: prev.total.modelRoundTrips + model_round_trips,
              toolCalls: {
                text_file:
                  prev.total.toolCalls.text_file + (tool_calls.text_file ?? 0),
                image: prev.total.toolCalls.image + (tool_calls.image ?? 0),
                web: prev.total.toolCalls.web + (tool_calls.web ?? 0),
                shell_or_other:
                  prev.total.toolCalls.shell_or_other +
                  (tool_calls.shell_or_other ?? 0),
                unknown:
                  prev.total.toolCalls.unknown + (tool_calls.unknown ?? 0),
              },
            };
            const bySession = { ...prev.bySession };
            if (session != null) {
              const cur = bySession[session] ?? {
                input: 0,
                output: 0,
                cached: 0,
                cacheWrite: 0,
                modelRoundTrips: 0,
                toolCalls: {
                  text_file: 0,
                  image: 0,
                  web: 0,
                  shell_or_other: 0,
                  unknown: 0,
                },
              };
              bySession[session] = {
                input: cur.input + input_tokens,
                output: cur.output + output_tokens,
                cached: cur.cached + cached_input_tokens,
                cacheWrite: cur.cacheWrite + cache_write_input_tokens,
                modelRoundTrips: cur.modelRoundTrips + model_round_trips,
                toolCalls: {
                  text_file:
                    cur.toolCalls.text_file + (tool_calls.text_file ?? 0),
                  image: cur.toolCalls.image + (tool_calls.image ?? 0),
                  web: cur.toolCalls.web + (tool_calls.web ?? 0),
                  shell_or_other:
                    cur.toolCalls.shell_or_other +
                    (tool_calls.shell_or_other ?? 0),
                  unknown: cur.toolCalls.unknown + (tool_calls.unknown ?? 0),
                },
              };
            }
            return { total, bySession };
          });
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
          if (!mounted) return;
          const pending = logBuffer.current;
          if (pending.length === 0) return;
          logBuffer.current = [];
          setLogs((prev) => {
            const next = prev.concat(pending);
            if (next.length > LOG_MAX) {
              const tail = next.slice(-LOG_KEEP);
              const retainedSessions = new Set(
                tail
                  .filter((entry) => entry.request && entry.session != null)
                  .map((entry) => entry.session as number),
              );
              const earlierRequests = new Map<number, LogEntry>();
              for (const entry of next.slice(0, -LOG_KEEP)) {
                if (
                  entry.request &&
                  entry.session != null &&
                  !retainedSessions.has(entry.session)
                ) {
                  earlierRequests.set(entry.session, entry);
                }
              }
              const protectedRequests = Array.from(
                earlierRequests.values(),
              ).slice(-LOG_KEEP);
              const retained = protectedRequests.concat(
                tail.slice(protectedRequests.length),
              );
              return [
                {
                  line: `… earlier log lines dropped (${LOG_KEEP} recent lines kept; request summaries retained) …`,
                  session: null,
                  label: null,
                  level: "info",
                  t: Date.now(),
                },
                ...retained,
              ];
            }
            return next;
          });
        }, LOG_FLUSH_INTERVAL);
      }

      if (mounted) setListenersReady(true);
      return unlisteners;
    }

    const setupPromise = setup();
    // If listener registration fails, the Generate button (gated on
    // listenersReady) would otherwise stay disabled forever with no feedback.
    // Surface the failure via the error state so the UI can render it.
    setupPromise.catch((e: unknown) => {
      if (!mounted) return;
      const message = e instanceof Error ? e.message : String(e);
      console.error("Failed to register Tauri event listeners:", e);
      terminalState.current = true;
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
      inputInterpretation?: string,
      diff?: boolean,
      variables?: Record<string, string>,
      extraInputs?: Record<string, string>,
      expectedProfileSnapshotId?: string,
      runParallelOverrides?: RunParallelOverrides | null,
    ) => {
      terminalState.current = false;
      setState({ kind: "extracting" });
      setLogs([]);
      // Also drop any lines still buffered from a previous run's dying
      // subprocesses, so they aren't flushed into this run's console.
      logBuffer.current = [];
      setUsage(EMPTY_USAGE);
      setRunStartedAt(Date.now());
      setPassTimes({});
      setReviewRouting(null);
      setProviderLimitNotices([]);
      stageSequence.current = 0;
      setStageHistory([]);
      try {
        const result = await invoke<PipelineResult>("run_pipeline", {
          paperPath,
          inputInterpretation: inputInterpretation ?? null,
          diff: diff ?? false,
          variables: variables ?? null,
          extraInputs: extraInputs ?? null,
          expectedProfileSnapshotId: expectedProfileSnapshotId ?? null,
          ...(runParallelOverrides ? { runParallelOverrides } : {}),
        });
        terminalState.current = true;
        setState({
          kind: "done",
          markdown: result.markdown,
          report: result.report,
          extractedText: result.extracted_text,
          runId: result.run_id ?? null,
        });
        setStageHistory((previous) =>
          previous.map((entry) =>
            entry.status === "active"
              ? { ...entry, status: "done" as const }
              : entry,
          ),
        );
      } catch (e: unknown) {
        const message = e instanceof Error ? e.message : String(e);
        terminalState.current = true;
        setState((prev) => ({
          kind: "error" as const,
          message,
          failedAt: prev.kind === "error" ? undefined : prev.kind,
        }));
        setStageHistory((previous) =>
          previous.map((entry) =>
            entry.status === "active"
              ? { ...entry, status: "failed" as const }
              : entry,
          ),
        );
      }
    },
    [],
  );

  const rerunPipeline = useCallback(
    async (
      runId: string,
      opts?: { fromStep?: string; onlyFailed?: boolean },
    ) => {
      terminalState.current = false;
      setState({ kind: "extracting" });
      setLogs([]);
      // Also drop any lines still buffered from a previous run's dying
      // subprocesses, so they aren't flushed into this run's console.
      logBuffer.current = [];
      setUsage(EMPTY_USAGE);
      setRunStartedAt(Date.now());
      setPassTimes({});
      setReviewRouting(null);
      setProviderLimitNotices([]);
      stageSequence.current = 0;
      setStageHistory([]);
      try {
        const result = await invoke<PipelineResult>("rerun_run", {
          runId,
          fromStep: opts?.fromStep ?? null,
          onlyFailed: opts?.onlyFailed ?? false,
        });
        terminalState.current = true;
        setState({
          kind: "done",
          markdown: result.markdown,
          report: result.report,
          extractedText: result.extracted_text,
          runId: result.run_id ?? null,
        });
        setStageHistory((previous) =>
          previous.map((entry) =>
            entry.status === "active"
              ? { ...entry, status: "done" as const }
              : entry,
          ),
        );
      } catch (e: unknown) {
        const message = e instanceof Error ? e.message : String(e);
        terminalState.current = true;
        setState((prev) => ({
          kind: "error" as const,
          message,
          failedAt: prev.kind === "error" ? undefined : prev.kind,
        }));
        setStageHistory((previous) =>
          previous.map((entry) =>
            entry.status === "active"
              ? { ...entry, status: "failed" as const }
              : entry,
          ),
        );
      }
    },
    [],
  );

  const cancel = useCallback(async () => {
    try {
      await invoke("cancel_pipeline");
    } catch {
      // ignore
    }
  }, []);

  const reset = useCallback(() => {
    terminalState.current = true;
    setState({ kind: "idle" });
    stageSequence.current = 0;
    setStageHistory([]);
    setReviewRouting(null);
    setProviderLimitNotices([]);
  }, []);

  return {
    state,
    logs,
    usage,
    startPipeline,
    rerunPipeline,
    cancel,
    reset,
    listenersReady,
    runStartedAt,
    passTimes,
    stageHistory,
    reviewRouting,
    providerLimitNotices,
  };
}
