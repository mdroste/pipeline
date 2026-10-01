import { invoke } from "@tauri-apps/api/core";
import type {
  PaperWithRevision,
  ResearchExecution,
  ResearchLedger,
  ResearchNote,
  WorkbenchSession,
} from "./workbenchTypes";

export interface ProjectRecord<T> {
  id: string;
  workspaceId: string;
  kind: string;
  revision: number;
  body: T;
  updatedAt: string;
}
export interface TaskCursor {
  updatedAt: string;
  id: string;
}
export interface TaskPage {
  records: ProjectRecord<ResearchTask>[];
  nextCursor: TaskCursor | null;
}
export interface HomeSettings {
  manuscriptRevisionId: string | null;
  baselineExecutionId: string | null;
  briefNoteIds: string[];
  excludedNoteIds: string[];
  ignoredPaths: string[];
  layout: string;
  /** One optional milestone as YYYY-MM-DD; absent on records saved before it existed. */
  targetDate?: string | null;
  targetLabel?: string;
}
export interface FileEntry {
  path: string;
  hash: string | null;
  size: number;
  status: string;
  executable: boolean;
}
export interface Inventory {
  rootIdentity: string;
  files: FileEntry[];
  warnings: string[];
  capturedAt: string;
  complete: boolean;
}
export interface DocumentSelection {
  revisionId: string;
  revisionHash: string;
  start: number | null;
  end: number | null;
  page: number | null;
  region: [number, number, number, number] | null;
  quote: string;
}
export interface Annotation {
  selection: DocumentSelection;
  body: string;
  prefix: string;
  suffix: string;
  origin: string;
}
export interface ResearchTask {
  objective: string;
  anchorId: string | null;
  expectedOutputs: string[];
  expectedChecks: string[];
  status: string;
}
export interface Checkpoint {
  taskId: string;
  backend: string;
  rootIdentity: string;
  taskRoot: string;
  taskRootIdentity: string;
  base: Record<string, FileEntry>;
  proposed: Record<string, FileEntry>;
  state: string;
  instruction: string;
  git: Record<string, unknown> | null;
  limitations: string[];
  checkedExecutionIds: string[];
  capturedAt: string;
}
export interface AppliedFile {
  path: string;
  before: string | null;
  after: string | null;
  beforeExecutable: boolean;
  afterExecutable: boolean;
  claim: string;
  state: string;
}
export interface Application {
  checkpointId: string;
  rootIdentity: string;
  state: string;
  files: AppliedFile[];
  direction: string;
  error: string | null;
  checksValid: boolean;
}
export interface ProjectHome {
  settings: ProjectRecord<HomeSettings>;
  notes: ResearchNote[];
  noteHistory: Array<{
    noteId: string;
    details: { previous?: ResearchNote };
    createdAt: string;
  }>;
  tasks: ProjectRecord<ResearchTask>[];
  tasksCursor?: TaskCursor | null;
  anchors: ProjectRecord<Annotation>[];
  papers: PaperWithRevision[];
  executions: ResearchExecution[];
  ledger: ResearchLedger;
  inventory: ProjectRecord<Inventory> | null;
  changes: ProjectRecord<Checkpoint>[];
  applications: ProjectRecord<Application>[];
  contextPreview: string;
  workingCopyStatus: string;
  /** Files the current paper version depends on that changed or went missing. */
  workingCopyChanged?: number;
  fileAcceptance: boolean;
}
export type ProjectAction =
  | { action: "saveHome"; expectedRevision: number; settings: HomeSettings }
  | { action: "refresh" }
  | { action: "annotate"; selection: DocumentSelection; body: string }
  | {
      action: "createTask";
      objective: string;
      anchorId: string | null;
      expectedOutputs: string[];
      expectedChecks: string[];
    }
  | {
      action: "updateTask";
      taskId: string;
      expectedRevision: number;
      status: string;
    }
  | { action: "checkpoint"; taskId: string; paths: string[]; backend: string }
  | { action: "captureChanges"; checkpointId: string }
  | {
      action: "apply";
      checkpointId: string;
      paths: string[];
      expectedRevision: number;
    }
  | { action: "undo" | "recover"; applicationId: string }
  | { action: "reject"; checkpointId: string };
export interface DocumentView {
  revision: NonNullable<PaperWithRevision["revision"]>;
  title: string;
  text: string;
  start: number;
  end: number;
  totalBytes: number;
  pageCount: number | null;
  imageUrl: string | null;
  page: number | null;
}
export interface AnchorMapping {
  status: "exact" | "candidate" | "ambiguous" | "missing";
  original: DocumentSelection;
  targetRevisionId: string;
  candidates: [number, number][];
  explanation: string;
}
export interface SnapshotPreview {
  text: string | null;
  imageUrl: string | null;
  hash: string;
  size: number;
}
export interface HostExecutionPreview {
  profileId: string;
  fingerprint: string;
  command: string[];
  cwd: string;
  inputs: Record<string, unknown>;
  launch: Record<string, unknown>;
  outputs: string[];
  authorized: boolean;
  boundary: string;
}
export interface ProjectCapabilities {
  platform: string;
  fileAcceptance: boolean;
  git: string | null;
  latex: string | null;
  pdfPages: string | null;
  stataPolicy: string;
  qualification: string;
  record: string;
}
export const projectClient = {
  taskPage: (workspaceId: string, before: TaskCursor) =>
    invoke<TaskPage>("workbench_project_tasks", { workspaceId, before }),
  capabilities: () =>
    invoke<ProjectCapabilities>("workbench_project_capabilities"),
  workingFile: (workspaceId: string, path: string) =>
    invoke<SnapshotPreview>("workbench_working_file_preview", {
      workspaceId,
      path,
    }),
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
        "This operation has an uncertain outcome. Refresh the project and inspect its task and recovery history before trying again.",
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

export function changedPaths(checkpoint: Checkpoint): string[] {
  return [
    ...new Set([
      ...Object.keys(checkpoint.base),
      ...Object.keys(checkpoint.proposed),
    ]),
  ]
    .filter(
      (path) =>
        checkpoint.base[path]?.hash !== checkpoint.proposed[path]?.hash ||
        checkpoint.base[path]?.executable !==
          checkpoint.proposed[path]?.executable,
    )
    .sort();
}
/** DOM string offsets are UTF-16; stored locators are UTF-8 bytes. */
export function textSelection(
  text: string,
  start: number,
  end: number,
  view: DocumentView,
): DocumentSelection {
  const encoder = new TextEncoder();
  return {
    revisionId: view.revision.id,
    revisionHash: view.revision.contentHash,
    start: view.start + encoder.encode(text.slice(0, start)).length,
    end: view.start + encoder.encode(text.slice(0, end)).length,
    quote: text.slice(start, end),
    page: null,
    region: null,
  };
}
