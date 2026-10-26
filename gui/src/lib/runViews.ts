// Pure per-run view state for the review run queue. Each queued review owns a
// RunView built from its tagged `pipeline:*` events; the queue entry fills in
// what events cannot (status after a window reload, failure, cancellation).

import type {
  LogEntry,
  PassStatus,
  PassTiming,
  PipelineState,
  ProviderLimitNotice,
  ReviewRoutingSummary,
  RuntimeStage,
  RuntimeStageKind,
  TokenTotals,
  UsageState,
} from "../hooks/usePipeline";
import type { PipelineResult, QueuedRun, ToolCallCounts } from "./types";

export interface RunView {
  state: PipelineState;
  logs: LogEntry[];
  usage: UsageState;
  runStartedAt: number | null;
  passTimes: Record<string, PassTiming>;
  stageHistory: RuntimeStage[];
  reviewRouting: ReviewRoutingSummary | null;
  providerLimitNotices: ProviderLimitNotice[];
  /** Set once the run reached a terminal state. Tauri events and command
   *  responses travel separately, so a stage event still queued for delivery
   *  must not regress the finished view. */
  terminal: boolean;
  stageSequence: number;
}

/** Maximum log entries kept per run. */
export const LOG_MAX = 10000;
/** When truncating, keep this many entries. */
export const LOG_KEEP = 8000;

function emptyTotals(): TokenTotals {
  return {
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
}

export const EMPTY_USAGE: UsageState = { total: emptyTotals(), bySession: {} };

export function emptyRunView(startedAt: number | null = null): RunView {
  return {
    state: { kind: "idle" },
    logs: [],
    usage: EMPTY_USAGE,
    runStartedAt: startedAt,
    passTimes: {},
    stageHistory: [],
    reviewRouting: null,
    providerLimitNotices: [],
    terminal: false,
    stageSequence: 0,
  };
}

const STAGE_KINDS: readonly RuntimeStageKind[] = [
  "extracting",
  "orienting",
  "dispatching",
  "merging",
  "synthesizing",
  "done",
];

function isStageKind(value: unknown): value is RuntimeStageKind {
  return STAGE_KINDS.includes(value as RuntimeStageKind);
}

/** The progress state for a stage kind, with no pass results yet. */
function stateForStage(kind: RuntimeStageKind): PipelineState | null {
  switch (kind) {
    case "extracting":
    case "orienting":
      return { kind };
    case "dispatching":
    case "merging":
    case "synthesizing":
      return { kind, passes: {} };
    default:
      return null;
  }
}

export interface StagePayload {
  stage: string;
  id: string;
  label: string;
  stepIds: string[];
  stepLabels?: string[];
  mergeStepIds?: string[];
  mergeStepLabels?: string[];
  skipped?: boolean;
}

export function applyStage(view: RunView, payload: StagePayload): RunView {
  if (view.terminal || !isStageKind(payload.stage)) return view;
  const {
    stage,
    id,
    label,
    stepIds,
    stepLabels = [],
    mergeStepIds = [],
    mergeStepLabels = [],
    skipped = false,
  } = payload;
  const sequence = view.stageSequence + 1;
  const stageHistory = view.stageHistory.map((entry, index) =>
    index === view.stageHistory.length - 1 && entry.status === "active"
      ? { ...entry, status: "done" as const }
      : entry,
  );
  stageHistory.push({
    id: id || `runtime-${sequence}`,
    kind: stage,
    label,
    stepIds,
    stepLabels,
    mergeStepIds,
    mergeStepLabels,
    passes: {},
    status: skipped ? "skipped" : "active",
  });
  const next = skipped || stage === "done" ? null : stateForStage(stage);
  return {
    ...view,
    stageSequence: sequence,
    stageHistory,
    state: next ?? view.state,
  };
}

export function applyPass(
  view: RunView,
  payload: { name: string; status: string },
  now: number,
): RunView {
  const { name } = payload;
  const status = payload.status as PassStatus;
  let passTimes = view.passTimes;
  if (status !== "pending") {
    const current = passTimes[name];
    if (status === "running") {
      // First "running" marks the start; retries keep the original start.
      if (!current || current.end !== undefined)
        passTimes = { ...passTimes, [name]: { start: now } };
    } else {
      // done / error close out the pass.
      passTimes = {
        ...passTimes,
        [name]: { start: current?.start ?? now, end: now },
      };
    }
  }
  const state =
    view.state.kind === "dispatching" ||
    view.state.kind === "merging" ||
    view.state.kind === "synthesizing"
      ? { ...view.state, passes: { ...view.state.passes, [name]: status } }
      : view.state;
  const stageHistory = view.stageHistory.map((entry, index) =>
    index === view.stageHistory.length - 1 && entry.status === "active"
      ? { ...entry, passes: { ...entry.passes, [name]: status } }
      : entry,
  );
  return { ...view, passTimes, state, stageHistory };
}

export interface UsagePayload {
  session: number | null;
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens?: number;
  cache_write_input_tokens?: number;
  model_round_trips?: number;
  tool_calls?: Partial<ToolCallCounts>;
}

function addTotals(current: TokenTotals, payload: UsagePayload): TokenTotals {
  const tools = payload.tool_calls ?? {};
  return {
    input: current.input + payload.input_tokens,
    output: current.output + payload.output_tokens,
    cached: current.cached + (payload.cached_input_tokens ?? 0),
    cacheWrite: current.cacheWrite + (payload.cache_write_input_tokens ?? 0),
    modelRoundTrips: current.modelRoundTrips + (payload.model_round_trips ?? 0),
    toolCalls: {
      text_file: current.toolCalls.text_file + (tools.text_file ?? 0),
      image: current.toolCalls.image + (tools.image ?? 0),
      web: current.toolCalls.web + (tools.web ?? 0),
      shell_or_other:
        current.toolCalls.shell_or_other + (tools.shell_or_other ?? 0),
      unknown: current.toolCalls.unknown + (tools.unknown ?? 0),
    },
  };
}

export function applyUsage(view: RunView, payload: UsagePayload): RunView {
  const bySession = { ...view.usage.bySession };
  if (payload.session != null) {
    bySession[payload.session] = addTotals(
      bySession[payload.session] ?? emptyTotals(),
      payload,
    );
  }
  return {
    ...view,
    usage: { total: addTotals(view.usage.total, payload), bySession },
  };
}

export function applyProviderLimit(
  view: RunView,
  notice: ProviderLimitNotice,
): RunView {
  const notices = view.providerLimitNotices.filter(
    (existing) => existing.pass_key !== notice.pass_key,
  );
  notices.push(notice);
  return { ...view, providerLimitNotices: notices.slice(-20) };
}

/** Append flushed log lines, dropping the oldest beyond the cap while keeping
 *  every LLM request summary whose session would otherwise vanish. */
export function appendLogs(
  logs: LogEntry[],
  pending: LogEntry[],
  now = Date.now(),
): LogEntry[] {
  const next = logs.concat(pending);
  if (next.length <= LOG_MAX) return next;
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
  const protectedRequests = Array.from(earlierRequests.values()).slice(
    -LOG_KEEP,
  );
  const retained = protectedRequests.concat(
    tail.slice(protectedRequests.length),
  );
  return [
    {
      line: `… earlier log lines dropped (${LOG_KEEP} recent lines kept; request summaries retained) …`,
      session: null,
      label: null,
      level: "info",
      t: now,
    },
    ...retained,
  ];
}

function settleStages(
  stages: RuntimeStage[],
  status: "done" | "failed",
): RuntimeStage[] {
  return stages.map((entry) =>
    entry.status === "active" ? { ...entry, status } : entry,
  );
}

export function completeRun(view: RunView, result: PipelineResult): RunView {
  return {
    ...view,
    terminal: true,
    state: {
      kind: "done",
      markdown: result.markdown,
      report: result.report,
      extractedText: result.extracted_text,
      runId: result.run_id ?? null,
    },
    stageHistory: settleStages(view.stageHistory, "done"),
  };
}

/** `lastStage` stands in for the failing stage when no events were seen. */
export function failRun(
  view: RunView,
  message: string,
  lastStage?: string | null,
): RunView {
  if (view.state.kind === "error") return view;
  const fallback =
    isStageKind(lastStage) && lastStage !== "done" ? lastStage : undefined;
  return {
    ...view,
    terminal: true,
    state: {
      kind: "error",
      message,
      failedAt: view.state.kind === "idle" ? fallback : view.state.kind,
    },
    stageHistory: settleStages(view.stageHistory, "failed"),
  };
}

/** Bring a view in line with its queue entry. Covers runs first seen through
 *  the queue (after a window reload, events from before it are gone) and the
 *  terminal transitions that arrive only as queue status. */
export function syncWithQueue(
  view: RunView | undefined,
  run: QueuedRun,
): RunView {
  const current = view ?? emptyRunView(run.startedAt);
  switch (run.status) {
    case "queued":
      return current;
    case "running": {
      if (current.state.kind !== "idle") {
        return current.runStartedAt === null && run.startedAt !== null
          ? { ...current, runStartedAt: run.startedAt }
          : current;
      }
      const stage = isStageKind(run.stage) ? stateForStage(run.stage) : null;
      return {
        ...current,
        runStartedAt: current.runStartedAt ?? run.startedAt,
        state: stage ?? { kind: "extracting" },
      };
    }
    case "done":
      return current.terminal
        ? current
        : {
            ...current,
            terminal: true,
            stageHistory: settleStages(current.stageHistory, "done"),
          };
    case "failed":
      return failRun(current, run.error ?? "The review failed.", run.stage);
    case "cancelled":
      return failRun(
        current,
        run.startedAt === null
          ? "Cancelled before it started."
          : "Review cancelled.",
        run.stage,
      );
  }
}
