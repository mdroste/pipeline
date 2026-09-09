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
  parallelAgents?: string[];
  mergeAgent?: string | null;
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
