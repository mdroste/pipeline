import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  PipelineState,
  PassStatus,
  PassTiming,
  RuntimeStage,
  RuntimeStageKind,
} from "../hooks/usePipeline";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";

interface Props {
  state: PipelineState;
  runStartedAt?: number | null;
  passTimes?: Record<string, PassTiming>;
  plan?: ExecutionPlanStage[];
  stageHistory?: RuntimeStage[];
}

/** Human elapsed time from a millisecond span: 5 → "5s", 125 → "2m 5s". */
function fmtElapsed(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

const BASE_STAGES = [
  { key: "extracting", label: "Extract text" },
  { key: "orienting", label: "Build orientation map" },
  { key: "dispatching", label: "Run parallel steps" },
  { key: "merging", label: "Merge cross-agent reports" },
  { key: "synthesizing", label: "Run sequential steps" },
  { key: "done", label: "Complete" },
];

function formatPassName(key: string): string {
  // Handle "merge/stepId" keys
  if (key.startsWith("merge/")) {
    const base = key.slice(6);
    return `Merge: ${base.charAt(0).toUpperCase() + base.slice(1)}`;
  }
  // Handle "stepId/agentName" keys
  const slash = key.indexOf("/");
  if (slash === -1) {
    // Capitalize the step ID as a fallback label
    return key.charAt(0).toUpperCase() + key.slice(1).replace(/_/g, " ");
  }
  const base = key.slice(0, slash);
  const agent = key.slice(slash + 1);
  const label = base.charAt(0).toUpperCase() + base.slice(1).replace(/_/g, " ");
  return `${label} (${agent.charAt(0).toUpperCase() + agent.slice(1)})`;
}

function stageIndex(kind: string, stages: typeof BASE_STAGES): number {
  const idx = stages.findIndex((s) => s.key === kind);
  return idx === -1 ? -1 : idx;
}

type DisplayStatus = "done" | "active" | "pending" | "failed" | "skipped";

interface DisplayStage {
  id: string;
  kind: string;
  label: string;
  status: DisplayStatus;
  passes: Record<string, PassStatus>;
}

function historyPlan(history: RuntimeStage[]): ExecutionPlanStage[] {
  const counts: Partial<Record<RuntimeStageKind, number>> = {};
  const stages: ExecutionPlanStage[] = history.map((entry) => {
    const occurrence = (counts[entry.kind] ?? 0) + 1;
    counts[entry.kind] = occurrence;
    const fallbackLabel = entry.kind === "extracting"
      ? "Extract input"
      : entry.kind === "orienting"
        ? "Build orientation map"
        : entry.kind === "dispatching"
          ? `Parallel wave ${occurrence}`
          : entry.kind === "merging"
            ? `Merge parallel wave ${occurrence}`
            : entry.kind === "synthesizing"
              ? `Sequential step ${occurrence}`
              : "Complete";
    return {
      id: entry.id,
      kind: entry.kind,
      label: entry.label || fallbackLabel,
      stepIds: entry.stepIds,
    };
  });
  if (!stages.some((stage) => stage.kind === "done")) {
    stages.push({ id: "done", kind: "done", label: "Complete", stepIds: [] });
  }
  return stages;
}

function StatusDot({ status }: { status: DisplayStatus }) {
  if (status === "done") {
    return (
      <div className="w-5 h-5 rounded-full bg-green-600 flex items-center justify-center">
        <svg className="w-3 h-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
        </svg>
      </div>
    );
  }
  if (status === "failed") {
    return (
      <div className="w-5 h-5 rounded-full bg-red-600 flex items-center justify-center">
        <svg className="w-3 h-3 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M6 18L18 6M6 6l12 12" />
        </svg>
      </div>
    );
  }
  if (status === "active") {
    return (
      <div className="w-5 h-5 rounded-full border-2 border-gray-600 flex items-center justify-center">
        <div className="w-2 h-2 rounded-full bg-gray-600 animate-pulse" />
      </div>
    );
  }
  if (status === "skipped") {
    return (
      <div className="w-5 h-5 rounded-full border-2 border-gray-400 dark:border-gray-500 flex items-center justify-center">
        <span className="h-0.5 w-2 bg-gray-500 dark:bg-gray-400" />
      </div>
    );
  }
  return <div className="w-5 h-5 rounded-full border-2 border-gray-400 dark:border-gray-500" />;
}

function PassStatusIcon({ status }: { status: PassStatus }) {
  switch (status) {
    case "done":
      return <span className="text-green-700 dark:text-green-400 text-xs">done</span>;
    case "running":
      return <span className="text-gray-500 dark:text-gray-400 text-xs animate-pulse">running</span>;
    case "error":
      return <span className="text-red-600 dark:text-red-400 text-xs">failed</span>;
    case "skipped":
      return <span className="text-gray-500 dark:text-gray-400 text-xs italic">skipped</span>;
    default:
      return <span className="text-gray-500 dark:text-gray-400 text-xs">pending</span>;
  }
}

export default function PipelineProgress({
  state,
  plan,
  stageHistory = [],
  runStartedAt,
  passTimes,
}: Props) {
  const isError = state.kind === "error";
  const isCancelled = isError && state.message?.toLowerCase().includes("cancelled");
  const failedAtKey = isError ? (state.failedAt ?? "done") : state.kind;

  // Tick once a second while the run is active so elapsed clocks update live.
  const running = !isError && state.kind !== "done";
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [running]);

  const runElapsed = runStartedAt != null ? fmtElapsed(now - runStartedAt) : null;

  // Elapsed time for one pass: end−start if finished, else now−start if active.
  const passElapsed = (name: string): string | null => {
    const t = passTimes?.[name];
    if (!t) return null;
    const end = t.end ?? now;
    return fmtElapsed(end - t.start);
  };

  const mergeStageOrder = stageIndex("merging", BASE_STAGES);
  const currentBaseIdx = stageIndex(isError ? failedAtKey : state.kind, BASE_STAGES);
  const legacyPlan: ExecutionPlanStage[] = BASE_STAGES
    .filter((stage) =>
      stage.key !== "merging" ||
      state.kind === "merging" ||
      currentBaseIdx > mergeStageOrder
    )
    .map((stage) => ({
      id: stage.key,
      kind: stage.key as ExecutionPlanStage["kind"],
      label: stage.label,
      stepIds: [],
    }));
  const effectivePlan = plan?.length
    ? plan
    : stageHistory.length
      ? historyPlan(stageHistory)
      : legacyPlan;

  // Runtime stage IDs are emitted by the backend from the same scheduler that
  // produced the plan. Match on those IDs first so a guarded/skipped stage
  // cannot shift a later same-kind wave onto the wrong row.
  const runtimeByPlanIndex = new Map<number, RuntimeStage>();
  let legacySearchFrom = 0;
  let usedLegacyMatching = false;
  for (const runtime of stageHistory) {
    let index = effectivePlan.findIndex((candidate) => candidate.id === runtime.id);
    if (index === -1) {
      usedLegacyMatching = true;
      index = effectivePlan.findIndex(
        (candidate, candidateIndex) =>
          candidateIndex >= legacySearchFrom && candidate.kind === runtime.kind,
      );
    }
    if (index === -1) continue;
    runtimeByPlanIndex.set(index, runtime);
    legacySearchFrom = index + 1;
  }
  const furthestRuntimeIndex = runtimeByPlanIndex.size
    ? Math.max(...runtimeByPlanIndex.keys())
    : -1;

  let fallbackIndex = -1;
  if (stageHistory.length === 0 && state.kind !== "idle" && state.kind !== "done") {
    fallbackIndex = effectivePlan.findIndex((stage) => stage.kind === failedAtKey);
  }

  const stages: DisplayStage[] = effectivePlan.map((stage, index) => {
    const runtime = runtimeByPlanIndex.get(index);
    let status: DisplayStatus = "pending";
    if (runtime?.status === "skipped") {
      status = "skipped";
    } else if (state.kind === "done") {
      status = "done";
    } else if (runtime) {
      status = runtime.status === "active"
        ? "active"
        : runtime.status === "failed"
          ? "failed"
          : "done";
    } else if (usedLegacyMatching && index < furthestRuntimeIndex) {
      // No event means a planned stage was skipped (for example a guarded
      // step); a later event proves the executor advanced beyond it.
      status = "done";
    } else if (index === fallbackIndex) {
      status = isError ? "failed" : "active";
    } else if (fallbackIndex >= 0 && index < fallbackIndex) {
      status = "done";
    }
    const currentPasses =
      runtime?.passes ??
      (index === fallbackIndex && "passes" in state ? state.passes : {});
    return {
      id: stage.id,
      kind: stage.kind,
      label: stage.label,
      status,
      passes: currentPasses,
    };
  });

  return (
    <div className="space-y-1">
      <div className="flex items-baseline justify-between mb-2">
        <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">Progress</h3>
        {runElapsed && (
          <span className="text-xs text-gray-500 dark:text-gray-400 tabular-nums">{runElapsed}</span>
        )}
      </div>
      <div className="space-y-3">
        {stages.map((stage) => {
          const status = stage.status;
          return (
            <div key={stage.id} data-status={status}>
              <div className="flex items-center gap-2.5">
                <StatusDot status={status} />
                <span
                  className={`text-sm ${
                    status === "failed"
                      ? "text-red-600 dark:text-red-400 font-medium"
                      : status === "active"
                        ? "text-gray-900 dark:text-gray-100 font-medium"
                        : status === "done"
                          ? "text-gray-500 dark:text-gray-400"
                          : status === "skipped"
                            ? "text-gray-500 dark:text-gray-400"
                          : "text-gray-500 dark:text-gray-400"
                  }`}
                >
                  {stage.label}
                </span>
                {status === "failed" && (
                  <span className="text-red-600 dark:text-red-400 text-xs ml-auto">
                    {isCancelled ? "cancelled" : "failed"}
                  </span>
                )}
                {status === "active" && Object.keys(stage.passes).length === 0 && (
                  <span className="text-gray-500 dark:text-gray-400 text-xs animate-pulse ml-auto">running…</span>
                )}
                {status === "skipped" && (
                  <span className="text-gray-500 dark:text-gray-400 text-xs ml-auto">skipped</span>
                )}
              </div>

              {Object.keys(stage.passes).length > 0 && (
                  <div className="ml-7 mt-1.5 space-y-1">
                    {Object.entries(stage.passes).map(([name, passStatus]) => {
                      const el = passElapsed(name);
                      return (
                        <div
                          key={name}
                          className="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400"
                        >
                          <span>{formatPassName(name)}</span>
                          <span className="flex items-center gap-2">
                            {el && <span className="text-gray-500 dark:text-gray-400 tabular-nums">{el}</span>}
                            <PassStatusIcon status={passStatus} />
                            {passStatus === "running" && (
                              <button
                                type="button"
                                onClick={() => invoke("cancel_pass", { passKey: name }).catch(() => {})}
                                aria-label={`Cancel ${formatPassName(name)}`}
                                className="text-gray-600 hover:text-red-700 dark:text-gray-400 dark:hover:text-red-400 transition-colors"
                              >
                                ✕
                              </button>
                            )}
                          </span>
                        </div>
                      );
                    })}
                  </div>
                )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
