/** Studio request and record contracts; mutation results are selected by action. */
import type { ProjectRecord } from "./projectTypes";
import type {
  ExecutionProfile,
  ResearchExecution,
  ResearchResultV1,
} from "./workbench/executionTypes";
import type { PaperWithRevision } from "./workbench/researchTypes";
import type { Finding } from "./reviewTypes";
import type { DeskRecord } from "./deskTypes";
import type { Application, Checkpoint, ProjectHome } from "./projectTypes";
export type StudioKind =
  | "check_policy"
  | "check_state"
  | "connection"
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
  documentId?: string | null;
  name: string;
  checkpointId: string | null;
  directory: string;
  rootDocument: string;
  expectedPdf: string;
  engine: string;
  timeoutSeconds: number;
  inputs: string[];
  advancedArgv: string[] | null;
  automaticDependencies?: boolean;
  executable?: string | null;
}
export interface BuildRecord {
  recordType: "configuration";
  config: BuildConfig;
  profile: ExecutionProfile;
  localAuthorizationRequired: boolean;
}
export interface BuildReceipt {
  documentId?: string | null;
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
export interface SourceContextRequest {
  path: string;
  checkpointId: string | null;
  expectedHash: string | null;
  selection: {
    quote: string;
    start: number;
    end: number;
    line: number;
    page?: number | null;
    draft: boolean;
  } | null;
}
export interface ConnectionSettings {
  provider: "git" | "github" | "overleaf" | "dropbox";
  remoteName: string | null;
  webUrl: string | null;
  active: boolean;
}
export interface ProjectConnection extends ConnectionSettings {
  localRoot: string;
  rootIdentity: string;
  capabilities: string[];
  lastObservedAt: string;
  basis: string;
}
export type StudioAction =
  | {
      action: "saveConnection";
      expectedRevision: number;
      settings: ConnectionSettings;
    }
  | { action: "captureContext"; source: SourceContextRequest }
  | { action: "responseContext"; id: string; expectedRevision: number }
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

export interface CheckPolicy {
  enabled: boolean;
  sourcePath: string;
  bibliographies: string[];
}

/** SaveText preserves the distinct unchanged and changed native responses. */
export type SavedText =
  | { saved: true; hash: string; changed: false }
  | {
      saved: true;
      hash: string;
      checkpointId: string;
      application: ProjectRecord<Application> | null;
      checksValid: false;
    };

export interface StudioRecords {
  build: BuildRecord | BuildReceipt;
  response: ResponseRecord;
  connection: ProjectConnection;
  check_policy: CheckPolicy;
  check_state: NonNullable<ProjectHome["checks"]>[number]["body"];
  experiment: Experiment;
  specification: Specification;
  binding: NumericBinding;
  literature: LiteratureNote;
  theory: TheoryNote;
  check: TheoryCheck;
  direction: ResearchDirection;
  // Compatibility surfaces retain raw bodies until their own decoder is used.
  series: unknown;
  bibliography: unknown;
}

interface StudioResults {
  saveConnection: ProjectRecord<ProjectConnection>;
  captureContext: DeskRecord;
  responseContext: DeskRecord;
  createFile: {
    created: true;
    path: string;
    hash: string;
    bytes: number;
    checkpointId: string;
    application: ProjectRecord<Application> | null;
  };
  createFolder: { path: string };
  moveEntry: { from: string; to: string };
  trashEntry: { path: string };
  importFile: { path: string };
  saveText: SavedText;
  saveBuild: ProjectRecord<BuildRecord>;
  inspectBuild: BuildReceipt;
  inspectPages: BuildReceipt;
  importFindings: ProjectRecord<ResponseRecord>[];
  importReport: ProjectRecord<ResponseRecord>[];
  saveResponse: ProjectRecord<ResponseRecord>;
  saveExperiment: ProjectRecord<Experiment>;
  saveSpecification: ProjectRecord<Specification>;
  importResults: unknown;
  bindNumber: ProjectRecord<NumericBinding>;
  generateValues: ProjectRecord<Checkpoint>;
  importBibliography: unknown;
  saveLiterature: ProjectRecord<LiteratureNote>;
  saveTheory: ProjectRecord<TheoryNote>;
  saveCheck: ProjectRecord<TheoryCheck>;
  saveDirection: ProjectRecord<ResearchDirection>;
  convertDirection: unknown;
  promoteTheory: unknown;
}

export type StudioResult<A extends StudioAction> = StudioResults[A["action"]];
