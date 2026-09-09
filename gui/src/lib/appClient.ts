import { invoke } from "@tauri-apps/api/core";
import type {
  ExecutionPlanEnvelope,
  RunProfileSnapshot,
} from "./appRunPreparation";
import type {
  BatchJob,
  PipelineConfig,
  ProfileSummary,
  RunParallelOverrides,
} from "./types";

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

/** Typed boundary for application-shell commands owned by the pipeline runtime. */
export const appClient = {
  markSmokeReady: () => invoke<boolean>("mark_smoke_ready"),
  batchStatus: () => invoke<BatchJob[]>("get_batch_status"),
  runSetup: () => invoke<RunProfileSnapshot>("get_run_setup"),
  executionPlan: (request: ExecutionPlanRequest) =>
    invoke<ExecutionPlanEnvelope>("get_execution_plan", { ...request }),
  pipelineConfig: () => invoke<PipelineConfig>("get_pipeline_config"),
  profiles: () => invoke<ProfileSummary[]>("list_profiles"),
  inputFiles: (dir: string) => invoke<string[]>("list_input_files", { dir }),
  startBatch: (request: StartBatchRequest) =>
    invoke<void>("start_batch", { ...request }),
};
