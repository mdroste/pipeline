import { invoke } from "@tauri-apps/api/core";
import type { ProjectRecord } from "./projectClient";
import type {
  ExecutionProfile,
  PaperWithRevision,
  ResearchExecution,
  ResearchResultV1,
  ReviewHandoff,
} from "./workbenchTypes";
import type { Finding } from "./types";
export type StudioKind =
  | "build"
  | "response"
  | "experiment"
  | "specification"
  | "series"
  | "binding"
  | "bibliography"
  | "literature"
  | "theory"
  | "check"
  | "direction";
export interface Promotion {
  checkpointId: string;
  path: string;
  noteRevision: number;
  promotedAt: string;
}
export interface TheoryNote {
  kind: string;
  title: string;
  statement: string;
  body: string;
  assumptions: string[];
  assumptionIds: string[];
  anchorIds: string[];
  relatedIds: string[];
  unresolvedSteps: string[];
  status: string;
  rejectionReason: string;
  origin: string;
  promotions: Promotion[];
  importedFrom?: string | null;
  importedStatus?: string | null;
}
export interface TheoryCheck {
  theoryId: string;
  method: string;
  outcome: string;
  summary: string;
  domain: string;
  tolerance: number | null;
  precision: number | null;
  executionId: string | null;
  recipeRunId: string | null;
  anchorIds: string[];
  claimsGenerality: boolean;
  origin: string;
}
export interface ResearchDirection {
  question: string;
  mechanism: string;
  closestKnownWork: string;
  minimalModelOrData: string;
  firstDiscriminatingTest: string;
  likelyFailureMode: string;
  nextAction: string;
  status: string;
  taskId: string | null;
  theoryIds: string[];
  dropReason: string;
}
export interface BuildConfig {
  name: string;
  checkpointId: string | null;
  directory: string;
  rootDocument: string;
  expectedPdf: string;
  engine: string;
  timeoutSeconds: number;
  inputs: string[];
  advancedArgv: string[] | null;
}
export interface BuildRecord {
  recordType: "configuration";
  config: BuildConfig;
  profile: ExecutionProfile;
  localAuthorizationRequired: boolean;
}
export interface BuildReceipt {
  recordType: "receipt";
  executionId: string;
  outcome: string;
  diagnostics: Array<{
    severity: string;
    message: string;
    path: string | null;
    line: number | null;
  }>;
  pdf: PaperWithRevision | null;
  mappingArtifactId: string | null;
  pageInspection: string;
  inspectedPages?: number[];
  synchronization: string;
  sourceManifest: unknown;
}
export interface SyncResult {
  result: string;
  scope: string;
  candidates: Array<{
    page?: number;
    line?: number;
    sourcePath?: string;
    x?: number;
    y?: number;
  }>;
}
export interface EditorFile {
  path: string;
  content: string;
  hash: string;
  checkpointId: string | null;
}
export interface FindingPackage {
  version: number;
  runId: string;
  sourceStepId: string;
  findings: Finding[];
}
export interface FindingPreview {
  package: FindingPackage;
  packageHash: string;
  notice: string;
}
export interface ReportPreview {
  comments: ReportComment[];
  /** Paragraphs in the report; more than `comments` when the split stopped. */
  paragraphs: number;
}
export interface ReportComment {
  number: string;
  start: number;
  end: number;
  text: string;
}
export interface ResponseDecision {
  number: string;
  category: string;
  severity: string;
  disposition: string;
  intendedResponse: string;
  taskId: string | null;
  manuscriptRevisionId: string | null;
  applicationId: string | null;
  executionId: string | null;
  evidenceAnchorIds: string[];
  draft: string;
  rationale: string;
  disputedPremise: string;
  counterargument: string;
  resolvingCheck: string;
  reportsAnalysisAdded: boolean;
}
export interface ResponseRecord {
  source: {
    kind: string;
    runId?: string;
    revisionId?: string;
    comment?: ReportComment;
    finding?: Finding;
  };
  decision: ResponseDecision;
  flags: string[];
}
export interface Experiment {
  question: string;
  baselineExecutionId: string;
  intendedChange: string;
  executionIds: string[];
  interpretation: string;
}
export interface Specification {
  specificationId: string;
  sampleId: string;
  fields: Record<string, { value: unknown; origin: string; source: string }>;
}
export interface ResultRef {
  executionId: string;
  resultId: string;
}
export interface UnitConversion {
  fromUnits: string;
  toUnits: string;
  factor: number;
  offset: number;
  rationale: string;
}
export interface NumericBinding {
  result: ResultRef;
  anchorId: string;
  role: string;
  component: string;
  printed: string;
  precision: number;
  reportedUnits: string;
  origin: string;
  confirmed: boolean;
  importedFrom?: string | null;
  importedStatus?: string | null;
}
export interface LiteratureNote {
  question: string;
  statement: string;
  citationKey: string;
  sourceVersionId: string | null;
  start: number | null;
  end: number | null;
  quote: string;
  identityChecked: boolean;
  support: string;
  method: string;
  relatedVersionIds: string[];
}
export interface JobStatus {
  execution: ResearchExecution;
  state: string;
  owner: "turn" | "detached" | "unknown";
  durationSeconds: number | null;
  canReconcile: boolean;
  resource: string;
}
export interface ResponseExport {
  text: string;
  warnings: string[];
  draft: boolean;
  format: string;
  assessment: string;
}
export interface Comparison {
  comparable: boolean;
  blockers: string[];
  specificationDifferences: string[];
  left: ResearchResultV1;
  rightConverted: ResearchResultV1;
  leftSpecification: ProjectRecord<Specification> | null;
  rightSpecification: ProjectRecord<Specification> | null;
  signedChange: number | null;
  absoluteChange: number | null;
  relativeChange: number | null;
  nChange: number | null;
  conversion: UnitConversion | null;
  rationale: string;
}
export type StudioAction =
  | { action: "createFile"; checkpointId: null; path: string; base64: string }
  | { action: "createFolder"; path: string }
  | { action: "moveEntry"; from: string; to: string }
  | { action: "trashEntry"; path: string }
  | { action: "importFile"; directory: string; source: string }
  | {
      action: "saveText";
      checkpointId: string | null;
      path: string;
      expectedHash: string;
      content: string;
    }
  | {
      action: "saveBuild";
      id: string | null;
      expectedRevision: number;
      config: BuildConfig;
    }
  | { action: "inspectBuild"; executionId: string }
  | {
      action: "inspectPages";
      executionId: string;
      pages: number[];
      note: string;
    }
  | { action: "importFindings"; package: FindingPackage; selected: string[] }
  | { action: "importReport"; revisionId: string; comments: ReportComment[] }
  | {
      action: "saveResponse";
      id: string;
      expectedRevision: number;
      response: ResponseDecision;
    }
  | {
      action: "saveExperiment";
      id: string | null;
      expectedRevision: number;
      experiment: Experiment;
    }
  | {
      action: "saveSpecification";
      id: string | null;
      expectedRevision: number;
      specification: Specification;
    }
  | { action: "importResults"; executionId: string; artifactId: string }
  | {
      action: "bindNumber";
      id: string | null;
      expectedRevision: number;
      binding: NumericBinding;
    }
  | {
      action: "generateValues";
      checkpointId: string;
      path: string;
      values: { name: string; result: ResultRef; precision: number }[];
    }
  | { action: "importBibliography"; revisionId: string; selected: string[] }
  | {
      action: "saveLiterature";
      id: string | null;
      expectedRevision: number;
      note: LiteratureNote;
    }
  | {
      action: "saveTheory";
      id: string | null;
      expectedRevision: number;
      note: TheoryNote;
    }
  | {
      action: "saveCheck";
      id: string | null;
      expectedRevision: number;
      check: TheoryCheck;
    }
  | {
      action: "saveDirection";
      id: string | null;
      expectedRevision: number;
      direction: ResearchDirection;
    }
  | { action: "convertDirection"; id: string; expectedRevision: number }
  | {
      action: "promoteTheory";
      checkpointId: string;
      path: string;
      theoryId: string;
      afterLine: number | null;
    };
export const studioClient = {
  mutate: async <T>(workspaceId: string, action: StudioAction): Promise<T> => {
    const response = await invoke<T | { outcome: string }>(
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
    return response as T;
  },
  records: <T>(workspaceId: string, kind: StudioKind) =>
    invoke<ProjectRecord<T>[]>("workbench_studio_records", {
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
