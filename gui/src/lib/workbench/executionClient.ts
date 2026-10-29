import { invoke } from "@tauri-apps/api/core";
import type {
  ExecutionProfile,
  ResearchExecution,
  ResearchResultV1,
  ResultComparison,
  VerificationRecord,
} from "../workbenchTypes";

export const executionClient = {
  saveExecutionProfile(request: {
    profileId: string | null;
    workspaceId: string;
    name: string;
    adapter: ExecutionProfile["adapter"];
    argv: string[];
    cwd: string;
    environment: Record<string, string>;
    inputs: string[];
    timeoutSeconds: number;
    outputs: string[];
    expectedRevision: number | null;
    operationId: string;
  }) {
    return invoke<ExecutionProfile>("workbench_save_execution_profile", {
      request,
    });
  },
  listExecutionProfiles(workspaceId: string) {
    return invoke<ExecutionProfile[]>("workbench_list_execution_profiles", {
      workspaceId,
    });
  },
  runExecution(request: {
    planId?: string;
    profileId: string;
    sessionId: string | null;
    testOnly: boolean;
    operationId: string;
  }) {
    return invoke<ResearchExecution>("workbench_run_execution", { request });
  },
  listExecutions(workspaceId: string) {
    return invoke<ResearchExecution[]>("workbench_list_executions", {
      workspaceId,
    });
  },
  cancelExecution(executionId: string) {
    return invoke<void>("workbench_cancel_execution", {
      request: { executionId },
    });
  },
  compareResults(
    left: ResearchResultV1,
    right: ResearchResultV1,
    absoluteTolerance: number,
    rationale: string | null,
  ) {
    return invoke<ResultComparison>("workbench_compare_results", {
      request: { left, right, absoluteTolerance, rationale },
    });
  },
  recordStructuredResult(
    workspaceId: string,
    result: ResearchResultV1,
    operationId: string,
  ) {
    return invoke<ResearchResultV1>("workbench_record_structured_result", {
      request: { workspaceId, result, operationId },
    });
  },
  listStructuredResults(workspaceId: string) {
    return invoke<ResearchResultV1[]>("workbench_list_structured_results", {
      workspaceId,
    });
  },
  verifyEvidenceResults(request: {
    evidenceId: string;
    leftResultId: string;
    rightResultId: string;
    absoluteTolerance: number;
    rationale: string | null;
    operationId: string;
  }) {
    return invoke<VerificationRecord>("workbench_verify_evidence_results", {
      request,
    });
  },
};
