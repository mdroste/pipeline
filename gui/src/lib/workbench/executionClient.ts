import { invoke } from "@tauri-apps/api/core";
import type { ExecutionProfile, ResearchExecution } from "../workbenchTypes";
import type { ToolchainReport } from "./toolchains";

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
  /** Read-only: standard install folders, PATH and the project folder. */
  detectToolchains(workspaceId: string | null) {
    return invoke<ToolchainReport>("workbench_detect_toolchains", {
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
  executionOutput(workspaceId: string, executionId: string) {
    return invoke<Pick<ResearchExecution, "stdout" | "stderr">>(
      "workbench_execution_output",
      { workspaceId, executionId },
    );
  },
  cancelExecution(executionId: string) {
    return invoke<void>("workbench_cancel_execution", {
      request: { executionId },
    });
  },
};
