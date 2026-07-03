// --- Step-based pipeline types ---

export type Phase = "parallel" | "sequential";

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
  /** Per-step effort override; empty/undefined = use the global setting. */
  effort?: string;
}

export interface MergeConfig {
  enabled: boolean;
  prompt: string;
  agents: string[];
}

/** Per-profile extraction overrides. Empty/null fields inherit from global Settings. */
export interface ExtractionConfig {
  /** "" | "auto" | "llm" | "marker" | "pdftotext". Empty = inherit. */
  method: string;
  /** null = inherit; true/false = override. */
  marker_disable_ocr: boolean | null;
  marker_disable_images: boolean | null;
}

export interface PipelineConfig {
  steps: StepConfig[];
  merge: MergeConfig;
  use_orientation: boolean;
  /** Custom orientation-map prompt. Empty = use the default (prompts/orientation.md). */
  orientation_prompt: string;
  extraction: ExtractionConfig;
  parallel_context_template: string;
}

export interface StepOutput {
  step_id: string;
  step_label: string;
  phase: string;
  agent: string;
  raw_text: string;
}

// --- Report types ---

export interface StepFailure {
  step_id: string;
  step_label: string;
  error: string;
}

export interface PipelineReport {
  orientation: OrientationMap;
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

export interface PipelineResult {
  report: PipelineReport;
  markdown: string;
  extracted_text: string;
}

export interface Settings {
  preferred_provider: string;
  max_workers: number;
  active_profile: string;
  claude_model: string;
  claude_effort: string;
  codex_model: string;
  codex_effort: string;
  gemini_model: string;
  pdf_extractor: string;
  marker_disable_ocr: boolean;
  marker_disable_images: boolean;
  verbose_logging: boolean;
  step_timeout_secs: number;
  max_retries: number;
  anthropic_api_key: string;
  openai_api_key: string;
  google_api_key: string;
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
      name: string;
      steps: StepConfig[];
      merge: MergeConfig;
      use_orientation?: boolean;
      orientation_prompt?: string;
      extraction?: ExtractionConfig;
      parallel_context_template?: string;
    }
  | { type: "bundle"; settings: Settings; profiles: ProfileExport[]; active_profile: string };
