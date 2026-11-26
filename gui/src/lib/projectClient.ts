import { invoke } from "@tauri-apps/api/core";
import type { WorkbenchSession } from "./workbench/sessionTypes";
import type {
  EarlierRecords,
  EarlierRecordsExport,
  TaskCursor,
  TaskPage,
  ProjectCapabilities,
  SnapshotPreview,
  ProjectHome,
  ProjectAction,
  ProjectRecord,
  DocumentView,
  AnchorMapping,
  HostExecutionPreview,
} from "./projectTypes";
export type * from "./projectTypes";
export { changedPaths, textSelection } from "./projectSelection";

export const projectClient = {
  earlierRecords: (workspaceId: string) =>
    invoke<EarlierRecords>("workbench_earlier_records", { workspaceId }),
  exportEarlierRecords: (workspaceId: string, destination: string) =>
    invoke<EarlierRecordsExport>("workbench_export_earlier_records", {
      workspaceId,
      destination,
    }),
  taskPage: (workspaceId: string, before: TaskCursor) =>
    invoke<TaskPage>("workbench_project_tasks", { workspaceId, before }),
  capabilities: () =>
    invoke<ProjectCapabilities>("workbench_project_capabilities"),
  workingFile: (workspaceId: string, path: string) =>
    invoke<SnapshotPreview>("workbench_working_file_preview", {
      workspaceId,
      path,
    }),
  chatBrief: (workspaceId: string) =>
    invoke<string | null>("workbench_chat_brief", { workspaceId }),
  home: (workspaceId: string) =>
    invoke<ProjectHome>("workbench_project_home", { workspaceId }),
  mutate: async <T = unknown>(
    workspaceId: string,
    action: ProjectAction,
  ): Promise<ProjectRecord<T>> => {
    const result = await invoke<ProjectRecord<T> | { outcome: string }>(
      "workbench_project_mutate",
      {
        request: {
          workspaceId,
          operationId: `project-${crypto.randomUUID()}`,
          ...action,
        },
      },
    );
    if ("outcome" in result)
      throw new Error(
        "Pipeline could not confirm whether this finished. Refresh the project and check its task history and recovery records before trying again.",
      );
    return result;
  },
  read: (
    workspaceId: string,
    revisionId: string,
    start = 0,
    page: number | null = null,
  ) =>
    invoke<DocumentView>("workbench_document_read", {
      workspaceId,
      revisionId,
      start,
      page,
    }),
  map: (workspaceId: string, anchorId: string, revisionId: string) =>
    invoke<AnchorMapping>("workbench_anchor_mapping", {
      workspaceId,
      anchorId,
      revisionId,
    }),
  preview: (workspaceId: string, hash: string) =>
    invoke<SnapshotPreview>("workbench_snapshot_preview", {
      workspaceId,
      hash,
    }),
  taskSession: (workspaceId: string, checkpointId: string) =>
    invoke<WorkbenchSession>("workbench_task_session", {
      workspaceId,
      checkpointId,
    }),
  previewExecution: (profileId: string) =>
    invoke<HostExecutionPreview>("workbench_preview_host_execution", {
      profileId,
    }),
  authorizeExecution: (profileId: string, fingerprint: string) =>
    invoke<HostExecutionPreview>("workbench_authorize_host_execution", {
      profileId,
      fingerprint,
    }),
};
