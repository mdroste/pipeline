import type { OrientationMap } from "./reviewOrientationTypes";
import type { NamedProductSpec } from "./workflowTypes";

/** Provider-neutral breakdown of model-issued tool calls. */
export interface ToolCallCounts {
  text_file: number;
  image: number;
  web: number;
  shell_or_other: number;
  unknown: number;
}

export interface StepOutput {
  step_id: string;
  step_label: string;
  /** Explicit identity for sibling agent reports that should be merged. */
  merge_group?: string;
  /** Source item bound to `{item}` for a fan-out unit. */
  fan_out_item?: string;
  phase: string;
  agent: string;
  raw_text: string;
  /** Whether raw_text is a schema-validated standalone JSON artifact. */
  structured_json?: boolean;
  /** Host-rendered view of a validated structured editorial record. */
  rendered_markdown?: string;
  /** Wall-clock seconds the step's LLM call took (0 for pre-1.1 reports). */
  duration_secs?: number;
  input_tokens?: number;
  output_tokens?: number;
  cached_input_tokens?: number;
  cache_write_input_tokens?: number;
  /** Provider-reported model generations inside this step. */
  model_round_trips?: number;
  /** Provider-reported tool calls inside this step. */
  tool_calls?: ToolCallCounts;
  attempt_count?: number;
  /** Effective model id/alias used for this step. */
  model?: string;
  /** Provider that ran this step. */
  provider?: string;
  model_transport?: string;
  model_policy?: string;
  model_source?: string;
  model_catalog_updated_at?: string;
  calls?: StepCallRecord[];
  /** True when the step was skipped by its run_if guard. */
  skipped?: boolean;
}

export interface StepCallRecord {
  role?: string;
  provider?: string;
  agent?: string;
  model?: string;
  model_transport?: string;
  model_policy?: string;
  model_source?: string;
  model_catalog_updated_at?: string;
  /**
   * How the app knows `model`: reported by the provider, requested by
   * Pipeline, expected (the CLI chose and did not say), or unknown.
   */
  model_basis?: "reported" | "requested" | "expected" | "unknown" | "";
  /** Requested thinking level. Empty when the provider's default applied. */
  effort?: string;
  duration_secs?: number;
  input_tokens?: number;
  output_tokens?: number;
  cached_input_tokens?: number;
  cache_write_input_tokens?: number;
  model_round_trips?: number;
  tool_calls?: ToolCallCounts;
  attempt_count?: number;
}

// --- Report types ---

export interface StepFailure {
  step_id: string;
  step_label: string;
  /** Execution phase; absent on reports saved before phase-aware failures. */
  phase?: string;
  error: string;
}

export interface PipelineReport {
  /**
   * Survey JSON built before the steps ran. Raw — any schema the profile's
   * survey prompt produces. Paper-review profiles produce OrientationMap;
   * use isPaperOrientation() before treating it as one.
   */
  orientation: OrientationMap | Record<string, unknown> | null;
  step_outputs: StepOutput[];
  failed_steps?: StepFailure[];
  products?: RunProducts;
  quality?: ReportQuality;
  // Legacy fields for old saved reports
  referee_reports?: RefereeReport[];
  editor?: EditorSynthesis | null;
  report_date: string;
  paper_hash: string;
}

export interface FindingEvidence {
  evidence_type?: string;
  verification_status?: string;
  page?: number;
  line_start?: number;
  line_end?: number;
  node_id?: string;
  asset_id?: string;
  artifact_path?: string;
  source_path?: string;
  source_hash?: string;
  url?: string;
  doi?: string;
  publisher?: string;
  accessed_at?: string;
  query_id?: string;
  call_id?: string;
  description?: string;
  quote?: string;
}

export interface Finding {
  rank?: number;
  id: string;
  source_key?: string;
  /** Report ids of the reviewer analyses that support this finding. */
  sources?: string[];
  reviewer_ids?: string[];
  source_call_ids?: string[];
  title: string;
  category?: string;
  priority?: string;
  confidence?: string;
  verification_status?: string;
  problem?: string;
  consequence?: string;
  recommended_action?: string;
  body: string;
  evidence?: FindingEvidence[];
}

export interface FindingSet {
  schema_version: number;
  source_step_id: string;
  source_step_label: string;
  taxonomy?: string[];
  findings: Finding[];
}

export interface RunProducts {
  schema_version: number;
  primary_step_id: string;
  findings?: FindingSet | null;
  validation_dispositions?: ValidationDisposition[];
  named?: NamedRunProduct[];
}

export interface NamedRunProduct extends Omit<
  NamedProductSpec,
  "step" | "schema"
> {
  source_step_id: string;
  content: unknown;
}

export interface ValidationDisposition {
  finding_id: string;
  disposition: string;
  reason: string;
  merged_into?: string;
  before_hash?: string;
  after_hash?: string;
}

export interface ReportQuality {
  schema_version: number;
  status: string;
  extraction_method: string;
  input_bytes: number;
  orientation_included_bytes: number;
  orientation_omitted_bytes: number;
  recovered_pages: number;
  recovered_blocks: number;
  recovered_assets: number;
  extraction_notes?: string[];
  coverage_notes?: string[];
  completed_steps?: string[];
  failed_steps?: string[];
  skipped_steps?: string[];
  requested_capabilities?: string[];
  observed_capabilities?: string[];
  external_providers?: string[];
  schema_retry_events: number;
  verified_evidence: number;
  partially_verified_evidence: number;
  unverified_evidence: number;
  not_checkable_evidence?: number;
  manual_review_items: number;
  limitations?: string[];
  reproducibility_hash: string;
}

// Legacy types (for reading old saved reports)
export interface RefereeReport {
  pass_name: string;
  pass_label: string;
  agent: string;
  steelman: string;
  assessment: string;
  raw_text: string;
}

export interface EditorSynthesis {
  overall_assessment: string;
}

/** Mirrors runs::RunSummary — one row in the run-history list. */
export interface RunSummary {
  run_id: string;
  created: string;
  input_name: string;
  input_path: string;
  input_mode: string;
  input_interpretation?: string;
  input_identity?: InputIdentity;
  profile_id: string;
  profile_name: string;
  workflow_source?: string;
  workflow_fingerprint?: string;
  specialist_catalog_revision?: string;
  provider: string;
  status: string;
  duration_secs: number;
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens: number;
  cache_write_input_tokens: number;
  model_round_trips?: number;
  tool_calls?: ToolCallCounts;
  step_count: number;
  artifact_count: number;
  failed_steps: string[];
  resumable: boolean;
  title: string;
  tags: string[];
}

export interface InputIdentity {
  review_handoff_id?: string | null;
  selection_key: string;
  content_hash: string;
  lineage_id: string;
}

/** Mirrors runs::RunsDiskUsage. Trashed runs use disk space until deleted. */
export interface RunsDiskUsage {
  count: number;
  bytes: number;
  trash_count: number;
  trash_bytes: number;
}

/** A recoverable run currently held outside History. */
export interface TrashedRun {
  run_id: string;
  deleted_at: string;
  title: string;
  input_name: string;
}

/** Mirrors commands::BatchJob — one input in a batch run. */
export interface BatchJob {
  path: string;
  name: string;
  status: string; // pending | running | done | partial | failed | cancelled | interrupted
  run_id: string | null;
  error: string | null;
  duration_secs: number;
  profile_id?: string;
  profile_snapshot_id?: string;
}

export type QueuedRunStatus =
  "queued" | "running" | "done" | "failed" | "cancelled";

/** One review in the run queue (`runs:queue`, `list_run_queue`). */
export interface QueuedRun {
  key: string;
  kind: "review" | "rerun";
  label: string;
  inputPath: string;
  profileName: string;
  status: QueuedRunStatus;
  /** Latest runtime stage kind and label while running. */
  stage: string | null;
  stageLabel: string | null;
  /** Why a queued review cannot start yet beyond waiting its turn. */
  waiting: string | null;
  queuedAt: number;
  startedAt: number | null;
  finishedAt: number | null;
  runId: string | null;
  error: string | null;
  /** Finished, but one or more steps failed. */
  partial: boolean;
  /** The full result is still held by the queue (recent runs only). */
  resultAvailable: boolean;
}

export interface RunQueueSnapshot {
  /** Reviews that may run at the same time. */
  limit: number;
  runs: QueuedRun[];
}

export interface PipelineResult {
  report: PipelineReport;
  markdown: string;
  extracted_text: string;
  /** Present when the run directory was written; keys the artifact explorer. */
  run_id?: string | null;
}
