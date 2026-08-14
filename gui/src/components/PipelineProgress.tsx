import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  PipelineState,
  PassStatus,
  PassTiming,
  RuntimeStage,
  RuntimeStageKind,
  ReviewRoutingSummary,
} from "../hooks/usePipeline";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";

interface Props {
  state: PipelineState;
  runStartedAt?: number | null;
  passTimes?: Record<string, PassTiming>;
  plan?: ExecutionPlanStage[];
  stageHistory?: RuntimeStage[];
  reviewRouting?: ReviewRoutingSummary | null;
}

/** Human elapsed time from a millisecond span: 5 → "5s", 125 → "2m 5s". */
function fmtElapsed(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

const BASE_STAGES = [
  { key: "extracting", label: "Creating document bundle" },
  { key: "orienting", label: "Creating orientation map" },
  { key: "dispatching", label: "Parallel agent wave" },
  { key: "merging", label: "Merge cross-agent reports" },
  { key: "synthesizing", label: "Sequential agent wave" },
  { key: "done", label: "Complete" },
];

function titleize(key: string): string {
  return key.charAt(0).toUpperCase() + key.slice(1).replace(/_/g, " ");
}

function passStepId(key: string): string {
  if (key.startsWith("merge/")) return key.slice(6).split("/")[0];
  return key.split("/")[0];
}

function formatPassName(key: string, stepLabels = new Map<string, string>()): string {
  // Handle "merge/stepId" keys
  if (key.startsWith("merge/")) {
    const base = key.slice(6);
    const stepId = base.split("/")[0];
    return `Merge: ${stepLabels.get(stepId) || titleize(base)}`;
  }
  // Handle "stepId/agentName" keys
  const slash = key.indexOf("/");
  if (slash === -1) {
    return stepLabels.get(key) || titleize(key);
  }
  const base = key.slice(0, slash);
  const agent = key.slice(slash + 1);
  const label = stepLabels.get(base) || titleize(base);
  return `${label} (${titleize(agent)})`;
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
  subitems: DisplaySubitem[];
}

interface DisplaySubitem {
  key: string;
  label: string;
  status: PassStatus;
  passKey?: string;
}

function historyPlan(history: RuntimeStage[]): ExecutionPlanStage[] {
  const counts: Partial<Record<RuntimeStageKind, number>> = {};
  const stages: ExecutionPlanStage[] = history.map((entry) => {
    const occurrence = (counts[entry.kind] ?? 0) + 1;
    counts[entry.kind] = occurrence;
    const fallbackLabel = entry.kind === "extracting"
      ? "Creating document bundle"
      : entry.kind === "orienting"
        ? "Creating orientation map"
        : entry.kind === "dispatching"
          ? "Parallel agent wave"
          : entry.kind === "merging"
            ? `Merge parallel wave ${occurrence}`
            : entry.kind === "synthesizing"
              ? "Sequential agent wave"
              : "Complete";
    return {
      id: entry.id,
      kind: entry.kind,
      label: entry.label || fallbackLabel,
      stepIds: entry.stepIds,
      stepLabels: entry.stepLabels,
    };
  });
  if (!stages.some((stage) => stage.kind === "done")) {
    stages.push({ id: "done", kind: "done", label: "Complete", stepIds: [] });
  }
  return stages;
}

function waveLabel(kind: string, fallback: string): string {
  if (kind === "dispatching") return "Parallel agent wave";
  if (kind === "synthesizing") return "Sequential agent wave";
  return fallback;
}

function statusForPlannedSubitem(status: DisplayStatus): PassStatus {
  if (status === "done") return "done";
  if (status === "failed") return "error";
  if (status === "skipped") return "skipped";
  return "pending";
}

function statusForProcessingSubitem(status: DisplayStatus): PassStatus {
  if (status === "active") return "running";
  return statusForPlannedSubitem(status);
}

function routedAdaptiveSpecialists(routing: ReviewRoutingSummary): Array<{
  id: string;
  label: string;
}> {
  const grouped = [
    { kind: "subject", ids: routing.subjectIds ?? [], labels: routing.subjectLabels ?? [] },
    { kind: "method", ids: routing.methodIds ?? [], labels: routing.methodLabels ?? [] },
  ];
  if (grouped.some(({ ids, labels }) => ids.length > 0 || labels.length > 0)) {
    return grouped.flatMap(({ kind, ids, labels }) =>
      Array.from({ length: Math.max(ids.length, labels.length) }, (_, index) => {
        const id = ids[index] || `${kind}-${index}`;
        return { id, label: labels[index] || titleize(id) };
      }),
    );
  }
  return Array.from(
    { length: Math.max(routing.specialistIds.length, routing.specialistLabels.length) },
    (_, index) => {
      const id = routing.specialistIds[index] || `specialist-${index}`;
      return { id, label: routing.specialistLabels[index] || titleize(id) };
    },
  );
}

function processingLabel(stage: DisplayStage): string {
  if (stage.kind === "orienting") return stage.label || "Creating orientation map";
  if (/source.?tree|inventory/i.test(stage.label)) return "Creating source-tree inventory";
  if (/workflow context|prepare run/i.test(stage.label)) return "Preparing workflow context";
  return "Creating document bundle";
}

function combinedProcessingStatus(stages: DisplayStage[]): DisplayStatus {
  if (stages.some((stage) => stage.status === "failed")) return "failed";
  if (stages.some((stage) => stage.status === "active")) return "active";
  if (stages.every((stage) => stage.status === "done" || stage.status === "skipped")) {
    return "done";
  }
  if (stages.some((stage) => stage.status === "done" || stage.status === "skipped")) {
    return "active";
  }
  return "pending";
}

function combinedSequentialStatus(stages: DisplayStage[]): DisplayStatus {
  if (stages.some((stage) => stage.status === "failed")) return "failed";
  if (stages.some((stage) => stage.status === "active")) return "active";
  if (stages.every((stage) => stage.status === "skipped")) return "skipped";
  if (stages.every((stage) => stage.status === "done" || stage.status === "skipped")) {
    return "done";
  }
  if (stages.some((stage) => stage.status === "done" || stage.status === "skipped")) {
    return "active";
  }
  return "pending";
}

function groupAdjacentSequentialStages(stages: DisplayStage[]): DisplayStage[] {
  const grouped: DisplayStage[] = [];
  for (const stage of stages) {
    const previous = grouped[grouped.length - 1];
    if (stage.kind === "synthesizing" && previous?.kind === "synthesizing") {
      previous.status = combinedSequentialStatus([previous, stage]);
      previous.subitems.push(...stage.subitems);
    } else {
      grouped.push({ ...stage, subitems: [...stage.subitems] });
    }
  }
  return grouped;
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
  reviewRouting,
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
  const isAdaptiveReview = Boolean(reviewRouting) || effectivePlan.some(
    (stage) =>
      stage.stepIds.includes("auto_synthesis") ||
      (stage.kind === "orienting" && /review plan/i.test(stage.label)),
  );
  const adaptiveWaveIndex = isAdaptiveReview
    ? effectivePlan.findIndex((stage) => stage.kind === "dispatching")
    : -1;

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

  const plannedStages: DisplayStage[] = effectivePlan.map((stage, index) => {
    const runtime = runtimeByPlanIndex.get(index);
    // The runtime dispatch stage is authoritative about which materialized
    // steps are multi-provider. Its merge metadata is available before the
    // merge stage itself begins, including for adaptive steps unknown at
    // preflight time.
    const dispatchForMerge = stage.kind === "merging" && index > 0
      ? runtimeByPlanIndex.get(index - 1)
      : undefined;
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
    const stepIds = runtime?.stepIds?.length
      ? runtime.stepIds
      : dispatchForMerge?.mergeStepIds?.length
        ? dispatchForMerge.mergeStepIds
        : stage.stepIds;
    const stepLabelValues = runtime?.stepLabels?.length
      ? runtime.stepLabels
      : dispatchForMerge?.mergeStepLabels?.length
        ? dispatchForMerge.mergeStepLabels
        : stage.stepLabels ?? [];
    const stepLabels = new Map(
      stepIds.map((stepId, stepIndex) => [stepId, stepLabelValues[stepIndex] || titleize(stepId)]),
    );
    const coveredStepIds = new Set<string>();
    const subitems: DisplaySubitem[] = Object.entries(currentPasses).map(([name, passStatus]) => {
      coveredStepIds.add(passStepId(name));
      return {
        key: name,
        label: formatPassName(name, stepLabels),
        status: passStatus,
        passKey: name,
      };
    });
    for (const stepId of stepIds) {
      if (coveredStepIds.has(stepId)) continue;
      const label = stepLabels.get(stepId) || titleize(stepId);
      subitems.push({
        key: `planned/${stepId}`,
        label: stage.kind === "merging" ? `Merge: ${label}` : label,
        status: statusForPlannedSubitem(status),
      });
    }
    if (index === adaptiveWaveIndex) {
      const plannedStepIds = new Set(stage.stepIds);
      const routedSpecialists = reviewRouting
        ? routedAdaptiveSpecialists(reviewRouting)
        : [];
      const routedIds = new Set(routedSpecialists.map(({ id }) => id));
      const runtimeHasAdaptiveSpecialists = runtime?.stepIds.some(
        (stepId) => routedIds.has(stepId) || !plannedStepIds.has(stepId),
      ) ?? false;
      if (!runtimeHasAdaptiveSpecialists) {
        if (reviewRouting) {
          for (const specialist of routedSpecialists) {
            subitems.push({
              key: `adaptive/${specialist.id}`,
              label: specialist.label,
              status: statusForPlannedSubitem(status),
            });
          }
        } else {
          subitems.push({
            key: "adaptive/pending",
            label: "Adaptive agents",
            status: statusForPlannedSubitem(status),
          });
        }
      }
    }
    return {
      id: stage.id,
      kind: stage.kind,
      label: waveLabel(stage.kind, stage.label),
      status,
      subitems,
    };
  });

  // Extraction/normalization and the required orientation call are separate
  // scheduler stages, but they are one user-facing phase. Preserve each
  // stage's status as a subitem so failures still point to the exact work.
  const processingStages = plannedStages.filter(
    (stage) => stage.kind === "extracting" || stage.kind === "orienting",
  );
  let stages = plannedStages;
  if (processingStages.length > 0) {
    const processingIndex = plannedStages.findIndex(
      (stage) => stage.kind === "extracting" || stage.kind === "orienting",
    );
    const processingStage: DisplayStage = {
      id: "processing-inputs",
      kind: "processing",
      label: "Processing inputs",
      status: combinedProcessingStatus(processingStages),
      subitems: processingStages.map((stage) => ({
        key: stage.id,
        label: processingLabel(stage),
        status: statusForProcessingSubitem(stage.status),
      })),
    };
    stages = plannedStages.filter(
      (stage) => stage.kind !== "extracting" && stage.kind !== "orienting",
    );
    stages.splice(processingIndex, 0, processingStage);
  }
  stages = groupAdjacentSequentialStages(stages);

  return (
    <div className="space-y-1">
      <div className="flex items-baseline justify-between mb-2">
        <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300">Progress</h3>
        {runElapsed && (
          <span className="text-xs text-gray-500 dark:text-gray-400 tabular-nums">{runElapsed}</span>
        )}
      </div>

      {reviewRouting && (
        <div className="mb-3 rounded-lg border border-blue-200 bg-blue-50 px-3 py-2 dark:border-blue-900 dark:bg-blue-950/30">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-blue-700 dark:text-blue-300">
            Auto-detected review
          </p>
          <p className="mt-0.5 text-xs font-medium text-blue-950 dark:text-blue-100">
            {[reviewRouting.primaryDomain, reviewRouting.subject].filter(Boolean).join(" · ")}
          </p>
          {(reviewRouting.subjectLabels?.length ?? 0) > 0 && (
            <p className="mt-1 text-[11px] leading-4 text-blue-800 dark:text-blue-300">
              <span className="font-medium">Subject:</span> {reviewRouting.subjectLabels?.join(", ")}
            </p>
          )}
          {(reviewRouting.methodLabels?.length ?? 0) > 0 && (
            <p className="text-[11px] leading-4 text-blue-800 dark:text-blue-300">
              <span className="font-medium">Methods:</span> {reviewRouting.methodLabels?.join(", ")}
            </p>
          )}
          {!reviewRouting.subjectLabels && (
            <p className="mt-1 text-[11px] leading-4 text-blue-800 dark:text-blue-300">
              {reviewRouting.specialistLabels.join(", ")}
            </p>
          )}
        </div>
      )}
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
                {status === "active" && stage.subitems.length === 0 && (
                  <span className="text-gray-500 dark:text-gray-400 text-xs animate-pulse ml-auto">running…</span>
                )}
                {status === "skipped" && (
                  <span className="text-gray-500 dark:text-gray-400 text-xs ml-auto">skipped</span>
                )}
              </div>

              {stage.subitems.length > 0 && (
                  <div className="ml-7 mt-1.5 space-y-1">
                    {stage.subitems.map((subitem) => {
                      const el = subitem.passKey ? passElapsed(subitem.passKey) : null;
                      return (
                        <div
                          key={subitem.key}
                          className="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400"
                        >
                          <span>{subitem.label}</span>
                          <span className="flex items-center gap-2">
                            {el && <span className="text-gray-500 dark:text-gray-400 tabular-nums">{el}</span>}
                            <PassStatusIcon status={subitem.status} />
                            {subitem.status === "running" && subitem.passKey && (
                              <button
                                type="button"
                                onClick={() => invoke("cancel_pass", { passKey: subitem.passKey }).catch(() => {})}
                                aria-label={`Cancel ${subitem.label}`}
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
