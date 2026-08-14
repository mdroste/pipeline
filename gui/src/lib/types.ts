// --- Step-based pipeline types ---

export type Phase = "parallel" | "sequential";

export type ModelSelection =
  | { mode: "automatic" }
  | { mode: "role"; role: string }
  | { mode: "pinned"; model: string };

export interface ModelCatalogEntry {
  id: string;
  display_name: string;
  description: string;
  is_default: boolean;
  supported_efforts: string[];
  capabilities: string[];
  deprecated: boolean;
  replacement?: string | null;
  input_price_per_million?: number | null;
  output_price_per_million?: number | null;
}

export interface ModelRole {
  id: string;
  label: string;
  description: string;
  model: string;
}

export interface ModelCatalog {
  provider: string;
  transport: "cli" | "api";
  source: string;
  source_version: string;
  fetched_at: string;
  stale: boolean;
  warning?: string | null;
  default_model?: string | null;
  recommended_model?: string | null;
  models: ModelCatalogEntry[];
  roles: ModelRole[];
}

/** Deterministic guard controlling whether a step runs (mirrors RunCondition). */
export type RunCondition =
  | { kind: "output_matches"; step: string; pattern: string; negate?: boolean }
  | { kind: "survey_path"; pointer: string; equals?: unknown; exists?: boolean; contains?: unknown };

export type PrimaryArtifactPart = "text" | "structure" | "visuals" | "source";
export type NamedInputArtifactPart = "text" | "source";
export type StepArtifactPart = "report" | "files";

/** A logical artifact selection. Paths are resolved privately for each call. */
export type ArtifactSelector =
  | { kind: "primary"; parts: PrimaryArtifactPart[] }
  | { kind: "survey" }
  | { kind: "named_input"; key: string; parts: NamedInputArtifactPart[] }
  | { kind: "step"; step: string; parts: StepArtifactPart[]; glob?: string };

/** Exact artifact allowlist for a step. Empty means intentionally isolated. */
export interface StepContext {
  include: ArtifactSelector[];
}

export interface StepConfig {
  id: string;
  label: string;
  prompt: string;
  enabled: boolean;
  phase: Phase;
  tools: string[];
  agents: string[];
  /** Per-step model override; empty/undefined = use the global setting. */
  model?: string;
  /** Provider/transport-specific model policies (for example codex:cli). */
  model_overrides?: Record<string, ModelSelection>;
  /** Per-step effort override; empty/undefined = use the global setting. */
  effort?: string;
  effort_overrides?: Record<string, string>;
  /** Order-only dependencies. Upstream artifact selectors add dataflow edges automatically. */
  after?: string[];
  /** Exact artifacts available to this step. */
  context: StepContext;
  /** Optional guard: skip the step unless this condition holds. */
  run_if?: RunCondition | null;
  /** Optional JSON-shape contract the step's output must satisfy (with retry). */
  output_schema?: Record<string, unknown> | null;
  /** Fan-out: run this step once per file matching the glob, with {item} bound. */
  for_each?: ForEach | null;
}

/** Fan-out (map) config for a step (mirrors ForEach). */
export interface ForEach {
  glob: string;
  max: number;
}

export interface MergeConfig {
  enabled: boolean;
  prompt: string;
  agents: string[];
}

/** An additional named input a profile accepts (mirrors InputSlot). */
export interface InputSlot {
  key: string;
  label?: string;
  /** "document" | "folder". */
  mode?: string;
  required?: boolean;
}

/** Run-time meaning assigned to the primary file-system selection. */
export type InputInterpretation =
  | "document"
  | "latex_project"
  | "source_tree"
  | "batch";

/** One primary selection may contain several paths when it represents a batch. */
export interface PrimaryInputSelection {
  paths: string[];
  interpretation: InputInterpretation;
  selectionKind: "file" | "folder";
}

/** Per-profile extraction configuration. Parser tuning lives in global Settings. */
export interface ExtractionConfig {
  /** "" | "auto" | "llm" | "paddleocr-vl-full" | "pdftotext". */
  method: string;
  /** "" or "document" | "folder" | "none". Empty = document. */
  input_mode?: string;
  /** Extra named inputs that steps may select; selected text resolves as {input:key}. */
  extra_inputs?: InputSlot[];
}

/** A run-time variable a profile declares (mirrors VarSpec). */
export interface VarSpec {
  key: string;
  label?: string;
  /** "text" | "choice" | "file". */
  kind?: string;
  default?: string;
  choices?: string[];
}

/** Optional, profile-scoped reuse of each step's selected primary text/survey. */
export interface ContextCacheConfig {
  enabled: boolean;
}

export interface PipelineConfig {
  steps: StepConfig[];
  merge: MergeConfig;
  /** Missing on older profiles/backends and therefore treated as disabled. */
  context_cache?: ContextCacheConfig;
  /** Compatibility field; the backend and editor normalize this to true. */
  use_orientation: boolean;
  /** Custom orientation-map prompt. Empty = use the default (prompts/orientation.md). */
  orientation_prompt: string;
  /** Optional JSON-shape contract for the orientation call. */
  orientation_schema?: Record<string, unknown> | null;
  extraction: ExtractionConfig;
  parallel_context_template: string;
  /** Run-time variables the profile declares; referenced in prompts as {var:key}. */
  variables?: VarSpec[];
}

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
  /** Resolved reasoning-effort setting requested for this call. */
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
  // Legacy fields for old saved reports
  referee_reports?: RefereeReport[];
  editor?: EditorSynthesis | null;
  report_date: string;
  paper_hash: string;
}

export interface OrientationMap {
  // The backend stores the survey as raw JSON and validates it only as an
  // object for schema-less profiles, so even a paper-shaped survey can omit
  // any of these fields; treat every access as optional.
  metadata?: PaperMetadata;
  sections: SectionEntry[];
  formal_results: FormalResult[];
  tables_figures: TableFigure[];
  notation: NotationEntry[];
  stated_contribution: string;
  key_references: string[];
  extraction_quality_notes: ExtractionQualityNote[];
  review_plan?: ReviewPlan | null;
}

export interface ReviewSelectionNote {
  id: string;
  reason: string;
}

export interface ReviewPlan {
  primary_domain: string;
  subject: string;
  paper_forms: string[];
  methods: string[];
  subject_specialist_ids?: string[];
  /** Legacy Auto Review v1 field selection retained for saved reports. */
  field_specialist_id?: string;
  method_specialist_ids: string[];
  /** Document-genre classification shared with every reviewer as context;
   *  absent on pre-genre saved reports and "research_article" by default. */
  genre?: string;
  selection_notes: ReviewSelectionNote[];
  routing_uncertainty: string[];
}

export interface AutoReviewCatalogRole {
  id: string;
  label: string;
  /** "discipline" and "family" mark a group's broad fallback role. */
  level: "discipline" | "subfield" | "family" | "method";
  description: string;
  exclusions: string;
}

/** A discipline of subject roles or a family of method roles. */
export interface AutoReviewCatalogGroup {
  id: string;
  label: string;
  roles: AutoReviewCatalogRole[];
}

export interface AutoReviewCatalog {
  contract: string;
  subjectCount: number;
  methodCount: number;
  disciplines: AutoReviewCatalogGroup[];
  methodFamilies: AutoReviewCatalogGroup[];
}

/** True when a survey JSON has the paper-review orientation shape. */
export function isPaperOrientation(o: unknown): o is OrientationMap {
  if (!o || typeof o !== "object") return false;
  const v = o as Record<string, unknown>;
  const meta = v.metadata as Record<string, unknown> | undefined;
  return (
    (typeof meta === "object" && meta !== null && "paper_type" in meta) ||
    "formal_results" in v ||
    "stated_contribution" in v
  );
}

export interface PaperMetadata {
  title?: string;
  authors?: string[];
  date: string | null;
  paper_type: "theory" | "empirical" | "mixed";
  page_count: number | null;
  has_appendix: boolean;
  has_online_appendix: boolean;
}

export interface SectionEntry {
  number: string;
  title: string;
  page_start: number | null;
  page_end: number | null;
}

export interface FormalResult {
  kind: string;
  number: string;
  page: number | null;
  summary: string;
  proof_location: string | null;
}

export interface TableFigure {
  kind: string;
  number: string;
  page: number | null;
  caption_summary: string;
  what_it_shows: string;
}

export interface NotationEntry {
  symbol: string;
  definition: string;
  page_introduced: number | null;
}

export interface ExtractionQualityNote {
  page_range: string;
  description: string;
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
  profile_id: string;
  profile_name: string;
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

/** Mirrors runs::RunsDiskUsage. */
export interface RunsDiskUsage {
  count: number;
  bytes: number;
}

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

export type ProjectIssueStatus = "open" | "addressed" | "dismissed" | "regressed";

export interface ProjectIssueEvidence {
  page?: number;
  line_start?: number;
  line_end?: number;
  node_id?: string;
  asset_id?: string;
  artifact_path?: string;
  description?: string;
  quote?: string;
}

/** One immutable observation of a project issue in a saved run. */
export interface ProjectIssueOccurrence {
  key: string;
  run_id: string;
  issue_id: string;
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

export interface PipelineResult {
  report: PipelineReport;
  markdown: string;
  extracted_text: string;
  /** Present when the run directory was written; keys the artifact explorer. */
  run_id?: string | null;
}

export interface Settings {
  preferred_provider: string;
  /** Providers inherited by Parallel steps whose workflow agent list is empty. */
  default_parallel_agents?: string[];
  default_parallel_model_overrides?: Record<string, ModelSelection>;
  default_parallel_effort_overrides?: Record<string, string>;
  /** Provider and policy used for multi-provider Parallel-output merges. */
  default_merge_agent?: string;
  default_merge_model_overrides?: Record<string, ModelSelection>;
  default_merge_effort_overrides?: Record<string, string>;
  /** Provider inherited by Sequential steps whose workflow agent list is empty. */
  default_sequential_agent?: string;
  default_sequential_model_overrides?: Record<string, ModelSelection>;
  default_sequential_effort_overrides?: Record<string, string>;
  /** Provider and policy used to build the orientation map. */
  default_orientation_agent?: string;
  default_orientation_model_overrides?: Record<string, ModelSelection>;
  default_orientation_effort_overrides?: Record<string, string>;
  /** Optional provider/model used once after durable account usage exhaustion. */
  usage_limit_fallback_agent?: string;
  usage_limit_fallback_model_overrides?: Record<string, ModelSelection>;
  usage_limit_fallback_effort_overrides?: Record<string, string>;
  max_workers: number;
  active_profile: string;
  claude_model: string;
  claude_cli_model_selection?: ModelSelection;
  claude_api_model_selection?: ModelSelection;
  claude_effort: string;
  codex_model: string;
  codex_cli_model_selection?: ModelSelection;
  codex_api_model_selection?: ModelSelection;
  codex_effort: string;
  antigravity_cli_model_selection?: ModelSelection;
  antigravity_api_model_selection?: ModelSelection;
  /** agy CLI reasoning effort: "" (default) | "low" | "medium" | "high". */
  antigravity_effort: string;
  /** "llm" | "auto" | "paddleocr-vl-full" | "pdftotext". */
  pdf_extractor: string;
  /** PaddleOCR-VL page slots; 0 = platform-aware automatic selection. */
  paddle_page_concurrency: number;
  /** llama.cpp multimodal encoder batch size; 0 = automatic. */
  paddle_mtmd_batch_tokens: number;
  /** "auto" | "on" | "off". */
  paddle_flash_attention: string;
  /** Maximum generated tokens for one OCR page. */
  paddle_max_output_tokens: number;
  /** Retries for a failed or suspicious OCR page. */
  paddle_page_retries: number;
  /** Official full-parser layout and document-restructuring controls. */
  paddle_full_layout_detection: boolean;
  paddle_full_layout_threshold: number;
  paddle_full_layout_nms: boolean;
  /** "large" | "small" | "union". */
  paddle_full_layout_merge_bboxes_mode: string;
  paddle_full_merge_layout_blocks: boolean;
  paddle_full_ocr_image_blocks: boolean;
  paddle_full_format_block_content: boolean;
  paddle_full_merge_tables: boolean;
  paddle_full_relevel_titles: boolean;
  paddle_full_show_formula_numbers: boolean;
  /** Wall-clock budget for the complete PDF extraction stage. */
  pdf_extraction_timeout_secs: number;
  /** Reuse versioned local extraction checkpoints. */
  reuse_pdf_extraction_cache: boolean;
  verbose_logging: boolean;
  step_timeout_secs: number;
  max_retries: number;
  /** Add an LLM revision reconciliation when a matching prior run exists. */
  auto_revision_reconciliation: boolean;
  /** Max past runs to keep on disk; 0 = keep all. */
  max_saved_runs: number;
  max_saved_run_bytes: number;
  /** Explicit cloud connection mode; subscription uses the provider CLI. */
  claude_access_mode: "subscription" | "api";
  codex_access_mode: "subscription" | "api";
  antigravity_access_mode: "subscription" | "api";
  anthropic_api_key: string;
  openai_api_key: string;
  google_api_key: string;
  /** Base URL of a local OpenAI-compatible server (Ollama default). */
  local_base_url: string;
  /** Model name on the local server; required for the local provider. */
  local_model: string;
  /** Optional bearer token for the local server. */
  local_api_key: string;
}

/** Transient New report override; never persisted to Settings. */
export interface RunParallelOverrides {
  agents: string[];
  model_overrides: Record<string, ModelSelection>;
  effort_overrides: Record<string, string>;
  merge_agent?: string;
  merge_model_overrides?: Record<string, ModelSelection>;
  merge_effort_overrides?: Record<string, string>;
}

export interface DepStatus {
  name: string;
  found: boolean;
  version: string;
  path: string;
  required: boolean;
  hint: string;
  /** Official platform setup documentation, when the dependency check recommends it. */
  help_url?: string;
  authenticated?: boolean;
  /** The installed CLI's own session, independent of direct-API readiness. */
  cli_auth_status?: "signed_in" | "signed_out" | "unknown";
}

export interface DepsReport {
  deps: DepStatus[];
  ready: boolean;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  update_available: boolean;
  release_url: string;
  release_name: string;
  published_at: string | null;
}

export interface ProfileSummary {
  id: string;
  name: string;
  step_count: number;
  builtin: boolean;
}

export interface ProfileExport {
  id: string;
  name: string;
  steps: StepConfig[];
  merge: MergeConfig;
  context_cache?: ContextCacheConfig;
  use_orientation?: boolean;
  orientation_prompt?: string;
  orientation_schema?: Record<string, unknown> | null;
  extraction?: ExtractionConfig;
  parallel_context_template?: string;
  variables?: VarSpec[];
}

export type ExportEnvelope =
  | { type: "step"; data: StepConfig }
  | {
      type: "profile";
      schema_version?: number;
      name: string;
      steps: StepConfig[];
      merge: MergeConfig;
      context_cache?: ContextCacheConfig;
      use_orientation?: boolean;
      orientation_prompt?: string;
      orientation_schema?: Record<string, unknown> | null;
      extraction?: ExtractionConfig;
      parallel_context_template?: string;
    }
  | { type: "bundle"; settings: Settings; profiles: ProfileExport[]; active_profile: string };

/** Mirrors engines::EngineStatus. */
export interface EngineInstallProgress {
  engine_id: string;
  phases: Record<string, string>;
  log_lines: string[];
}

export interface EngineStatus {
  id: string;
  label: string;
  description: string;
  installed: boolean;
  version: string;
  entry_path: string;
  system_path: string;
  est_download_mb: number;
  est_disk_mb: number;
  managed_stack_mb: number;
  installing: boolean;
  install_progress?: EngineInstallProgress | null;
  available?: boolean;
  unavailable_reason?: string;
}
