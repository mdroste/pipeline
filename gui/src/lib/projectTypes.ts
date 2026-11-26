/** Project wire records. Keep persisted field names and optional legacy fields stable. */
import type {
  PaperWithRevision,
  ResearchLedger,
  ResearchNote,
} from "./workbench/researchTypes";
import type { ResearchExecution } from "./workbench/executionTypes";

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
  /** Durable manuscript identity. Its newest capture is independent of the reference version. */
  primaryDocumentId?: string | null;
  /** Explicit historical reference; importing or building never advances it. */
  manuscriptRevisionId: string | null;
  baselineExecutionId: string | null;
  briefNoteIds: string[];
  excludedNoteIds: string[];
  ignoredPaths: string[];
  layout: string;
  /** One optional milestone as YYYY-MM-DD; absent on records saved before it existed. */
  targetDate?: string | null;
  targetLabel?: string;
  /** What sort of paper this is (`projectKinds`); absent until chosen. */
  kind?: string | null;
  /** Where the paper stands (`projectStages`); absent until chosen. */
  stage?: string | null;
  /** The journal, conference, or funder the paper is meant for. */
  venue?: string;
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
  checks?: ProjectRecord<{
    sourcePath: string;
    signature: string;
    outcome: string;
    receiptId: string;
    receiptHash: string;
    checkedAt: string;
    details: {
      missing?: string[];
      warnings?: string[];
      error?: string;
      coverage?: string;
    };
  }>[];
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
  | { action: "checkForChanges" }
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
  | {
      action: "acceptChanges";
      checkpointId: string;
      path: string;
      content: string;
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
/** Records kept by project views that have since been retired. */
export interface EarlierRecords {
  kinds: { kind: string; label: string; count: number }[];
  total: number;
}
export interface EarlierRecordsExport {
  directory: string;
  files: string[];
  records: number;
  notesCreated: number;
}
