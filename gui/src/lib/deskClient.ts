import { invoke } from "@tauri-apps/api/core";
export interface OpenResearchObject {
  kind: string;
  id: string;
  revision: string;
  start?: number | null;
  end?: number | null;
}
export interface ResearchObject {
  object: OpenResearchObject;
  title: string;
  text: string;
  provenance: string;
  access: string;
  completeness: string;
  truncated: boolean;
}
export interface DeskRecord<T = Record<string, unknown>> {
  id: string;
  workspaceId: string;
  kind: string;
  title: string;
  body: T;
  contentHash: string;
  supersedes: string | null;
  createdAt: string;
}
export interface ContextItem {
  role:
    | "main"
    | "source"
    | "data_dictionary"
    | "prior_draft"
    | "referee_report"
    | "result"
    | "supporting";
  object: OpenResearchObject;
}
export interface ContextSelection {
  revision: number;
  items: ContextItem[];
}
export interface IndexStatus {
  generation: number;
  complete: boolean;
  cursor: string;
  byteOffset: number;
}
export interface SearchPage {
  hits: ResearchObject[];
  nextCursor: string | null;
  index: IndexStatus;
  version: number;
}
export interface Relation {
  input: OpenResearchObject;
  dependent: OpenResearchObject;
  origin: string;
  accepted: boolean;
  reason: string;
}
export interface Decision {
  statement: string;
  rationale: string;
  alternatives: string[];
  assumptions: OpenResearchObject[];
  state: "proposed" | "accepted" | "rejected";
  noteId: string | null;
}
export interface ImpactReport {
  impacts: {
    object: OpenResearchObject;
    status: string;
    reason: string;
    path: OpenResearchObject[];
  }[];
  relations: Relation[];
  complete: boolean;
  limitations: string[];
}
export interface DataPolicy {
  dictionary: boolean;
  summaries: boolean;
  assistantRows: boolean;
  packageData: boolean;
}
export interface DataAcquisition {
  provider: string;
  source: string;
  retrievedAt: string;
  requestedVintage: string | null;
  returnedVintage: string | null;
  seriesIds: string[];
  units: string | null;
  frequency: string | null;
  transformation: string | null;
}
export interface VariableDefinition {
  name: string;
  inferredType: string;
  missing: number;
  units: string | null;
  description: string | null;
  origin: string;
  /** Variable label declared by the file, for example a Stata variable label. */
  label?: string | null;
  /** Declared storage type, for example `double`, `str12` or `strL`. */
  storageType?: string | null;
  /** Display or cell number format, for example `%td` or `yyyy-mm-dd`. */
  displayFormat?: string | null;
  /** Name of the attached value-label set. */
  valueLabels?: string | null;
  /** Counts per missing code, such as `.`, `.a`, `#N/A` or `empty`. */
  missingCodes?: Record<string, number>;
}
export interface ValueLabelSet {
  name: string;
  labels: { value: string; label: string }[];
  omitted?: number;
}
export interface SourceFormat {
  kind: string;
  release?: string | null;
  label?: string | null;
  timestamp?: string | null;
  sheet?: string | null;
  sheets?: string[];
  dateSystem?: string | null;
}
export type ConnectorId = "fred" | "dbnomics" | "world_bank";
export interface ConnectorProvenance {
  connector: ConnectorId;
  query: Record<string, unknown>;
  requestUrls: string[];
  lastUpdated: string | null;
  licence: string;
  notice: string | null;
  citation: string;
  termsUrl: string;
  termsReviewed: string;
}
export interface DatasetVersion {
  schemaVersion: number;
  artifactId: string | null;
  contentHash: string;
  externalReference: string | null;
  rows: number;
  columns: VariableDefinition[];
  acquisition: DataAcquisition;
  diagnosticsCoverage: string;
  delimiter: string;
  limitations: string[];
  valueLabels?: ValueLabelSet[];
  sourceFormat?: SourceFormat | null;
  connector?: ConnectorProvenance | null;
}
export type SeriesQuery =
  | { connector: "dbnomics"; seriesId: string }
  | {
      connector: "world_bank";
      indicator: string;
      countries: string;
      dateRange: string | null;
    };
export interface RowPreview {
  columns: string[];
  rows: (string | null)[][];
  start: number;
  totalRows: number;
  coverage: string;
  missingCodes?: { row: number; column: number; code: string }[];
}
export interface SampleDefinition {
  datasets: OpenResearchObject[];
  inclusionRules: string;
  filters: string[];
  weights: string | null;
  dateRange: string | null;
  unitOfObservation: string;
  membershipHash: string | null;
  origin: "declared";
}
export interface Candidate {
  title: string;
  doi: string | null;
  authors: string[];
  year: number | null;
  url: string | null;
  abstractText: string | null;
}
export interface Acquisition {
  provider: string;
  state: string;
  access?: string;
  query?: string;
  candidates?: Candidate[];
  error?: string;
  sourceId?: string;
  sourceVersionId?: string;
  canonicalUrl?: string;
}
export interface PlanStatus {
  record: DeskRecord;
  authorized: boolean;
  testStatus: string | null;
}
export const operation = () => `desk-${crypto.randomUUID()}`;
export const reference = (r: DeskRecord<unknown>): OpenResearchObject => ({
  kind: r.kind,
  id: r.id,
  revision: r.contentHash,
});
export const deskClient = {
  records: <T = Record<string, unknown>>(workspaceId: string, kind: string) =>
    invoke<DeskRecord<T>[]>("workbench_desk_records", { workspaceId, kind }),
  read: (workspaceId: string, object: OpenResearchObject) =>
    invoke<ResearchObject>("workbench_research_object", {
      workspaceId,
      object,
    }),
  context: (sessionId: string) =>
    invoke<ContextSelection>("workbench_context_selection", { sessionId }),
  saveContext: (
    sessionId: string,
    selection: ContextSelection,
    items: ContextItem[],
  ) =>
    invoke<ContextSelection>("workbench_save_context_selection", {
      sessionId,
      expectedRevision: selection.revision,
      items,
    }),
  collection: (
    workspaceId: string,
    title: string,
    objects: OpenResearchObject[],
    supersedes: string | null = null,
  ) =>
    invoke<DeskRecord>("workbench_reading_collection", {
      request: {
        workspaceId,
        title,
        objects,
        supersedes,
        operationId: operation(),
      },
    }),
  index: (workspaceId: string, rebuild = false) =>
    invoke<IndexStatus>("workbench_research_index", { workspaceId, rebuild }),
  search: (
    workspaceId: string,
    query: string,
    kind: string | null = null,
    cursor: string | null = null,
  ) =>
    invoke<SearchPage>("workbench_research_search", {
      request: { workspaceId, query, kind, cursor, limit: 20 },
    }),
  relation: (workspaceId: string, relation: Relation) =>
    invoke<DeskRecord>("workbench_save_relation", {
      workspaceId,
      relation,
      operationId: operation(),
    }),
  decision: (
    workspaceId: string,
    title: string,
    decision: Decision,
    supersedes: string | null = null,
  ) =>
    invoke<DeskRecord<Decision>>("workbench_save_decision", {
      workspaceId,
      title,
      decision,
      supersedes,
      operationId: operation(),
    }),
  impact: (workspaceId: string) =>
    invoke<ImpactReport>("workbench_change_impact", { workspaceId }),
  handoff: (sessionId: string) =>
    invoke<DeskRecord>("workbench_session_handoff", {
      sessionId,
      operationId: operation(),
    }),
  policy: (workspaceId: string, policy: DataPolicy | null = null) =>
    invoke<DataPolicy>("workbench_data_policy", { workspaceId, policy }),
  importDataset: (
    workspaceId: string,
    title: string,
    path: string,
    acquisition: DataAcquisition,
    supersedes: string | null = null,
    sheet: string | null = null,
  ) =>
    invoke<DeskRecord<DatasetVersion>>("workbench_import_dataset", {
      request: {
        workspaceId,
        title,
        path,
        acquisition,
        supersedes,
        operationId: operation(),
        sheet,
      },
    }),
  sample: (
    workspaceId: string,
    title: string,
    sample: SampleDefinition,
    supersedes: string | null = null,
  ) =>
    invoke<DeskRecord<SampleDefinition>>("workbench_save_sample", {
      workspaceId,
      title,
      sample,
      supersedes,
      operationId: operation(),
    }),
  network: (workspaceId: string, enabled: boolean | null = null) =>
    invoke<boolean>("workbench_acquisition_network", { workspaceId, enabled }),
  lookup: (workspaceId: string, query: string) =>
    invoke<DeskRecord<Acquisition>>("workbench_crossref_lookup", {
      workspaceId,
      query,
      operationId: operation(),
    }),
  candidate: (
    workspaceId: string,
    receiptId: string,
    index: number,
    citationKey: string | null,
  ) =>
    invoke<DeskRecord<Acquisition>>("workbench_acquire_candidate", {
      workspaceId,
      receiptId,
      index,
      citationKey,
      operationId: operation(),
    }),
  pdf: (workspaceId: string, title: string, url: string) =>
    invoke<DeskRecord<Acquisition>>("workbench_acquire_pdf", {
      workspaceId,
      title,
      url,
      operationId: operation(),
    }),
  fred: (
    workspaceId: string,
    seriesId: string,
    vintage: string,
    apiKey: string,
  ) =>
    invoke<DeskRecord>("workbench_acquire_fred", {
      request: {
        workspaceId,
        seriesId,
        vintage,
        apiKey,
        operationId: operation(),
      },
    }),
  /** Keyless DBnomics or World Bank capture; failures return an acquisition receipt. */
  series: (workspaceId: string, query: SeriesQuery) =>
    invoke<DeskRecord>("workbench_acquire_series", {
      request: { workspaceId, query, operationId: operation() },
    }),
  /** Read the optional Crossref contact email, or save it (`null` clears it). */
  crossrefContact: (email: string | null = null, save = false) =>
    invoke<string | null>("workbench_crossref_contact", { email, save }),
  capture: (
    profileId: string,
    parameters: Record<string, unknown>,
    randomSeed: string | null,
    toolchainVersion: string,
    researchInputs: OpenResearchObject[] = [],
  ) =>
    invoke<DeskRecord>("workbench_capture_execution_plan", {
      request: {
        profileId,
        parameters,
        randomSeed,
        toolchainVersion,
        researchInputs,
        operationId: operation(),
      },
    }),
  importMetadata: (
    workspaceId: string,
    title: string,
    version: DatasetVersion,
  ) =>
    invoke<DeskRecord<DatasetVersion>>("workbench_import_dataset_metadata", {
      request: { workspaceId, title, version, operationId: operation() },
    }),
  rows: (workspaceId: string, datasetId: string, start = 0) =>
    invoke<RowPreview>("workbench_dataset_rows", {
      workspaceId,
      datasetId,
      start,
    }),
  inboxState: (workspaceId: string, receiptId: string, state: string) =>
    invoke<DeskRecord>("workbench_reading_inbox_state", {
      workspaceId,
      receiptId,
      state,
      operationId: operation(),
    }),
  planStatus: (workspaceId: string, planId: string) =>
    invoke<PlanStatus>("workbench_execution_plan_status", {
      workspaceId,
      planId,
    }),
  authorizePlan: (workspaceId: string, planId: string, fingerprint: string) =>
    invoke<PlanStatus>("workbench_authorize_execution_plan", {
      workspaceId,
      planId,
      fingerprint,
    }),
};
