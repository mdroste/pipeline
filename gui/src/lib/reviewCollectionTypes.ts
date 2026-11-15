/** Local metadata grouping immutable runs into one continuing body of work. */
export interface Project {
  schema_version: number;
  id: string;
  name: string;
  description: string;
  created: string;
  updated: string;
  run_ids: string[];
}

export interface ProjectsResponse {
  projects: Project[];
  warnings: string[];
}

export interface TrashedProject {
  project: Project;
  deleted_at: string;
}

export type ProjectIssueStatus =
  "open" | "addressed" | "dismissed" | "regressed";

export interface ProjectIssueEvidence {
  page?: number;
  line_start?: number;
  line_end?: number;
  node_id?: string;
  asset_id?: string;
  artifact_path?: string;
  source_path?: string;
  source_hash?: string;
  description?: string;
  quote?: string;
}

/** One immutable observation of a project issue in a saved run. */
export interface ProjectIssueOccurrence {
  key: string;
  run_id: string;
  issue_id: string;
  source_key?: string;
  observed_at: string;
  profile_id: string;
  profile_name: string;
  input_name: string;
  input_mode: string;
  input_interpretation: string;
  step_id: string;
  step_label: string;
  title: string;
  severity: string;
  section: string;
  body: string;
  evidence: ProjectIssueEvidence[];
  annotation_status?: string;
  annotation_note?: string;
}

/** Durable identity and user decision spanning one or more run occurrences. */
export interface ProjectIssue {
  id: string;
  title: string;
  severity: string;
  section: string;
  status: ProjectIssueStatus;
  note: string;
  created: string;
  updated: string;
  decision_updated: string;
  occurrences: ProjectIssueOccurrence[];
  /** True when the issue was not observed in any readable run at the last
   * refresh; the decision and last-known occurrences are retained. */
  archived?: boolean;
}

export interface ProjectIssueLedger {
  schema_version: number;
  project_id: string;
  updated: string;
  issues: ProjectIssue[];
  warnings: string[];
}
