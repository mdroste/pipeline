import type {
  DepsReport,
  InputSlot,
  PipelineConfig,
  PrimaryInputSelection,
  RunParallelOverrides,
  VarSpec,
  BatchJob,
} from "./types";
import type { ExecutionPlanStage } from "./pipelineHelpers";
import type { UsageEstimate } from "./usageEstimate";
import type { HostCheck } from "./analyzers";

export interface RunProfileSnapshot {
  profileId: string;
  profileConfigSnapshotId: string;
  inputMode: string;
  variables: VarSpec[];
  inputSlots: InputSlot[];
}

export interface PendingRun extends RunProfileSnapshot {
  paperPath: string;
  inputSelection: PrimaryInputSelection | null;
}

export interface ExecutionPlanEnvelope {
  profileId: string;
  profileConfigSnapshotId: string;
  profileSnapshotId: string;
  configuredInputMode?: string;
  inputMode: string;
  inputInterpretation: string;
  variables: VarSpec[];
  inputSlots: InputSlot[];
  readiness: DepsReport;
  stages: ExecutionPlanStage[];
  /** Every provider the run may call, from the readiness check's own set. */
  providers?: string[];
  orientationAgent?: string;
  parallelAgents?: string[];
  sequentialAgent?: string;
  mergeAgent?: string | null;
  /** The usage-limit backup, when it couldn't run right now. */
  backupUnavailable?: string | null;
  usageEstimate?: UsageEstimate;
  hostChecks?: HostCheck[];
  /** Files in the selected folders that won’t fit in the Review’s copy. */
  inputNotices?: string[];
}

export interface PreparedLaunch {
  snapshot: PendingRun;
  variables?: Record<string, string>;
  extraInputs?: Record<string, string>;
  plan: ExecutionPlanEnvelope;
  config: PipelineConfig;
  parallelOverrides: RunParallelOverrides | null;
  batchPaths?: string[];
}

export function configForRunPreview(
  config: PipelineConfig,
  agents: string[],
  overrides: RunParallelOverrides | null,
  mergeAgent?: string | null,
  sequentialAgent?: string,
): PipelineConfig {
  return {
    ...config,
    merge: mergeAgent
      ? { ...config.merge, agents: [mergeAgent] }
      : config.merge,
    steps: config.steps.map((step) =>
      step.phase === "parallel" && (overrides || step.agents.length === 0)
        ? {
            ...step,
            agents,
            model: overrides ? "" : step.model,
            model_overrides: overrides?.model_overrides ?? step.model_overrides,
            effort: overrides ? "" : step.effort,
            effort_overrides:
              overrides?.effort_overrides ?? step.effort_overrides,
          }
        : step.phase === "sequential" &&
            step.agents.length === 0 &&
            sequentialAgent
          ? { ...step, agents: [sequentialAgent] }
          : step,
    ),
  };
}

export function plannedInterpretation(
  selection: PrimaryInputSelection | null,
): string | null {
  if (!selection) return null;
  if (selection.interpretation !== "batch") {
    return selection.interpretation;
  }
  return selection.selectionKind === "folder" ? "source_tree" : "document";
}

export function hasActiveBatch(jobs: BatchJob[]): boolean {
  return jobs.some(
    (job) => job.status === "pending" || job.status === "running",
  );
}
