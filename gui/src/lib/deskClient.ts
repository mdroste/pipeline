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
export interface Decision {
  statement: string;
  rationale: string;
  alternatives: string[];
  assumptions: OpenResearchObject[];
  state: "proposed" | "accepted" | "rejected";
  noteId: string | null;
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
export interface Candidate {
  title: string;
  doi: string | null;
  authors: string[];
  year: number | null;
  url: string | null;
  abstractText: string | null;
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
};
