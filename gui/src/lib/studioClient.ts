import { invoke } from "@tauri-apps/api/core";
import type { ProjectRecord } from "./projectTypes";
import type { ResearchExecution } from "./workbench/executionTypes";
import type { ReviewHandoff } from "./workbench/releaseTypes";
import type {
  StudioAction,
  StudioKind,
  StudioResult,
  StudioRecords,
  EditorFile,
  SyncResult,
  JobStatus,
  FindingPreview,
  FindingPackage,
  ReportPreview,
  ResponseExport,
} from "./studioTypes";
export type * from "./studioTypes";

export const studioClient = {
  exportFile: (args: { path: string; content: string }) =>
    invoke<void>("workbench_studio_export_file", args),
  mutate: async <A extends StudioAction>(
    workspaceId: string,
    action: A,
  ): Promise<StudioResult<A>> => {
    const response = await invoke<StudioResult<A> | { outcome: string }>(
      "workbench_studio_mutate",
      {
        request: {
          workspaceId,
          operationId: `studio-${crypto.randomUUID()}`,
          ...action,
        },
      },
    );
    if (
      response &&
      typeof response === "object" &&
      "outcome" in response &&
      response.outcome === "unknown"
    )
      throw new Error(
        "Pipeline could not confirm whether this finished. Refresh and check the saved records before trying again.",
      );
    return response as StudioResult<A>;
  },
  records: <K extends StudioKind>(workspaceId: string, kind: K) =>
    invoke<ProjectRecord<StudioRecords[K]>[]>("workbench_studio_records", {
      workspaceId,
      kind,
    }),
  history: (workspaceId: string, objectId: string) =>
    invoke<unknown[]>("workbench_studio_history", { workspaceId, objectId }),
  editor: (workspaceId: string, path: string, checkpointId: string | null) =>
    invoke<EditorFile>("workbench_editor_file", {
      workspaceId,
      path,
      checkpointId,
    }),
  externalEditor: (
    workspaceId: string,
    path: string,
    checkpointId: string | null,
  ) =>
    invoke<void>("workbench_editor_external", {
      workspaceId,
      path,
      checkpointId,
    }),
  sync: (
    workspaceId: string,
    executionId: string,
    request: {
      sourcePath?: string;
      line?: number;
      page?: number;
      x?: number;
      y?: number;
    },
  ) =>
    invoke<SyncResult>("workbench_build_sync", {
      request: { workspaceId, executionId, ...request },
    }),
  jobs: (workspaceId: string) =>
    invoke<JobStatus[]>("workbench_list_jobs", { workspaceId }),
  log: (workspaceId: string, executionId: string, stream: string, offset = 0) =>
    invoke<{ text: string; nextOffset: number; truncated: boolean }>(
      "workbench_job_log",
      { workspaceId, executionId, stream, offset },
    ),
  reconcile: (workspaceId: string, executionId: string) =>
    invoke<ResearchExecution>("workbench_reconcile_job", {
      workspaceId,
      executionId,
    }),
  workflowFindings: (runId: string) =>
    invoke<FindingPreview>("workbench_workflow_finding_preview", { runId }),
  findingPackage: (packageValue: FindingPackage) =>
    invoke<FindingPreview>("workbench_finding_package_preview", {
      package: packageValue,
    }),
  report: (workspaceId: string, revisionId: string) =>
    invoke<ReportPreview>("workbench_report_preview", {
      workspaceId,
      revisionId,
    }),
  exportResponses: (workspaceId: string, selected: string[], format: string) =>
    invoke<ResponseExport>("workbench_response_export", {
      workspaceId,
      selected,
      format,
    }),
  focusReview: (
    workspaceId: string,
    anchorIds: string[],
    dependencyAnchorIds: string[],
    responseIds: string[],
  ) =>
    invoke<ReviewHandoff>("workbench_focus_review", {
      request: { workspaceId, anchorIds, dependencyAnchorIds, responseIds },
    }),
};
