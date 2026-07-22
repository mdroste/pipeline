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
  | { kind: "survey_path"; pointer: string; equals?: unknown; exists?: boolean };

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
  /** Explicit upstream dependencies (step ids). Empty = implicit adjacency schedule. */
  inputs?: string[];
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

/** Per-profile extraction overrides. Empty/null fields inherit from global Settings. */
export interface ExtractionConfig {
  /** "" | "auto" | "llm" | "marker" | "pdftotext". Empty = inherit. */
  method: string;
  /** null = inherit; true/false = override. */
  marker_disable_ocr: boolean | null;
  marker_disable_images: boolean | null;
  /** "" or "document" | "folder" | "none". Empty = document. */
  input_mode?: string;
  /** Extra named inputs exposed to prompts as {input:key}. */
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

export interface PipelineConfig {
  steps: StepConfig[];
  merge: MergeConfig;
  use_orientation: boolean;
  /** Custom orientation-map prompt. Empty = use the default (prompts/orientation.md). */
  orientation_prompt: string;
  extraction: ExtractionConfig;
  parallel_context_template: string;
  /** Run-time variables the profile declares; referenced in prompts as {var:key}. */
  variables?: VarSpec[];
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
  attempt_count?: number;
  /** Effective model id/alias used for this step. */
  model?: string;
  /** Provider that ran this step. */
  provider?: string;
  model_transport?: string;
  model_policy?: string;
  model_source?: string;
  model_catalog_updated_at?: string;
  /** True when the step was skipped by its run_if guard. */
  skipped?: boolean;
}

// --- Report types ---

export interface StepFailure {
  step_id: string;
  step_label: string;
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
  metadata: PaperMetadata;
  sections: SectionEntry[];
  formal_results: FormalResult[];
  tables_figures: TableFigure[];
  notation: NotationEntry[];
  stated_contribution: string;
  key_references: string[];
  extraction_quality_notes: ExtractionQualityNote[];
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
  title: string;
  authors: string[];
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

export interface ReportSummary {
  paper_hash: string;
  title: string;
  report_date: string;
  file_path: string;
}

/** Mirrors runs::RunSummary — one row in the run-history list. */
export interface RunSummary {
  run_id: string;
  created: string;
  input_name: string;
  input_path: string;
  input_mode: string;
  profile_id: string;
  profile_name: string;
  provider: string;
  status: string;
  duration_secs: number;
  input_tokens: number;
  output_tokens: number;
  step_count: number;
  artifact_count: number;
  failed_steps: string[];
  title: string;
  tags: string[];
}

/** Mirrors runs::RunsDiskUsage. */
export interface RunsDiskUsage {
  count: number;
  bytes: number;
}

/** Mirrors commands::BatchJob — one input in a batch run. */
export interface BatchJob {
  path: string;
  name: string;
  status: string; // pending | running | done | failed | cancelled
  run_id: string | null;
  error: string | null;
  duration_secs: number;
  profile_id?: string;
  profile_snapshot_id?: string;
}

/** Mirrors commands::WatchStatus. */
export interface WatchStatus {
  active: boolean;
  folder: string;
  processed: BatchJob[];
  processed_total: number;
  failed_total: number;
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
  gemini_model: string;
  gemini_cli_model_selection?: ModelSelection;
  gemini_api_model_selection?: ModelSelection;
  pdf_extractor: string;
  marker_disable_ocr: boolean;
  marker_disable_images: boolean;
  verbose_logging: boolean;
  step_timeout_secs: number;
  max_retries: number;
  /** Max past runs to keep on disk; 0 = keep all. */
  max_saved_runs: number;
  max_saved_run_bytes: number;
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

export interface DepStatus {
  name: string;
  found: boolean;
  version: string;
  path: string;
  required: boolean;
  hint: string;
  authenticated?: boolean;
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
}

export type ExportEnvelope =
  | { type: "step"; data: StepConfig }
  | {
      type: "profile";
      schema_version?: number;
      name: string;
      steps: StepConfig[];
      merge: MergeConfig;
      use_orientation?: boolean;
      orientation_prompt?: string;
      extraction?: ExtractionConfig;
      parallel_context_template?: string;
    }
  | { type: "bundle"; settings: Settings; profiles: ProfileExport[]; active_profile: string };

/** Mirrors engines::EngineStatus. */
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
}
