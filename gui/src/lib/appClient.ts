import { invoke } from "@tauri-apps/api/core";
import type {
  ExecutionPlanEnvelope,
  RunProfileSnapshot,
} from "./appRunPreparation";
import type {
  BatchJob,
  PipelineConfig,
  PipelineResult,
  ProfileSummary,
  QueuedRun,
  RunParallelOverrides,
  RunQueueSnapshot,
} from "./types";

export interface RerunSecret {
  key: string;
  label: string;
}

export interface RerunRequirements {
  secrets: RerunSecret[];
  notice: string | null;
}

export interface ExecutionPlanRequest {
  variables: Record<string, string> | null;
  extraInputs: Record<string, string> | null;
  expectedProfileConfigSnapshotId: string | null;
  diff: boolean;
  paperPath: string | null;
  inputInterpretation: string | null;
  runParallelOverrides?: RunParallelOverrides;
}

export interface StartBatchRequest {
  paths: string[];
  variables: Record<string, string> | null;
  extraInputs: Record<string, string> | null;
  expectedProfileConfigSnapshotId: string;
  runParallelOverrides?: RunParallelOverrides;
}

export interface EnqueueRunRequest {
  paperPath: string;
  inputInterpretation: string | null;
  diff: boolean;
  variables: Record<string, string> | null;
  extraInputs: Record<string, string> | null;
  expectedProfileSnapshotId: string;
  runParallelOverrides?: RunParallelOverrides;
}

/** Typed boundary for application-shell commands owned by the pipeline runtime. */
export const appClient = {
  markSmokeReady: () => invoke<boolean>("mark_smoke_ready"),
  setSidebarMenuCollapsed: (collapsed: boolean) =>
    invoke<void>("set_sidebar_menu_collapsed", { collapsed }),
  batchStatus: () => invoke<BatchJob[]>("get_batch_status"),
  runSetup: () => invoke<RunProfileSnapshot>("get_run_setup"),
  executionPlan: (request: ExecutionPlanRequest) =>
    invoke<ExecutionPlanEnvelope>("get_execution_plan", { ...request }),
  pipelineConfig: () => invoke<PipelineConfig>("get_pipeline_config"),
  profiles: () => invoke<ProfileSummary[]>("list_profiles"),
  inputFiles: (dir: string) => invoke<string[]>("list_input_files", { dir }),
  startBatch: (request: StartBatchRequest) =>
    invoke<void>("start_batch", { ...request }),
  runQueue: () => invoke<RunQueueSnapshot>("list_run_queue"),
  enqueueRun: (request: EnqueueRunRequest) =>
    invoke<QueuedRun>("enqueue_run", { ...request }),
  enqueueBatch: (request: StartBatchRequest) =>
    invoke<QueuedRun[]>("enqueue_batch", { ...request }),
  enqueueRerun: (
    runId: string,
    onlyFailed: boolean,
    fromStep?: string,
    variables?: Record<string, string>,
  ) =>
    invoke<QueuedRun>("enqueue_rerun", {
      runId,
      fromStep: fromStep ?? null,
      onlyFailed,
      variables: variables ?? null,
    }),
  rerunRequirements: (runId: string, onlyFailed: boolean) =>
    invoke<RerunRequirements>("rerun_requirements", { runId, onlyFailed }),
  queuedRunResult: (key: string) =>
    invoke<PipelineResult | null>("get_queued_run_result", { key }),
  cancelRun: (runKey: string) => invoke<void>("cancel_pipeline", { runKey }),
  dismissRuns: (key?: string) =>
    invoke<void>("dismiss_queued_runs", { key: key ?? null }),
};
