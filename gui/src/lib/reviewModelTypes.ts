export type ModelSelection =
  | { mode: "automatic" }
  | { mode: "role"; role: string }
  | { mode: "pinned"; model: string };

export interface ModelCatalogEntry {
  id: string;
  display_name: string;
  description: string;
  is_default: boolean;
  /** Provider-reported default for this model, when available. */
  default_effort?: string | null;
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

/** Transient New report override; never persisted to Settings. */
/** The model a whole review runs on. An empty model lets the CLI or Pipeline choose. */
export interface ReviewModelChoice {
  provider: string;
  model: string;
  effort: string;
}

/**
 * One-report model choices (mirrors the backend's RunModelOverrides).
 * `review` sets the model every inheriting call runs on; `agents` and the
 * merge fields choose Parallel and Merge agents for this report.
 */
export interface RunModelOverrides {
  /** PDF reading method for this review only. */
  extraction_method?: string;
  review?: ReviewModelChoice;
  /** Settings stage exceptions use the review model for this report. */
  clear_settings_exceptions?: boolean;
  /** Workflow-pinned steps use the review model for this report. */
  override_workflow_pins?: boolean;
  agents: string[];
  model_overrides: Record<string, ModelSelection>;
  effort_overrides: Record<string, string>;
  merge_agent?: string;
  merge_model_overrides?: Record<string, ModelSelection>;
  merge_effort_overrides?: Record<string, string>;
}

/** Older name, kept for queued and batch launches. */
export type RunParallelOverrides = RunModelOverrides;

/** One group of calls in a review's model list (backend RosterEntry). */
export interface RosterEntry {
  stage:
    | "extraction"
    | "orientation"
    | "parallel"
    | "adaptive"
    | "merge"
    | "sequential"
    | "reconcile"
    | "fallback";
  label: string;
  step_ids: string[];
  step_labels: string[];
  provider: string;
  transport: string;
  model: ModelSelection;
  effort: string;
  source: "review" | "default" | "settings" | "workflow";
  note: string | null;
}
