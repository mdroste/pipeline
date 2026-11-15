import type { ModelSelection } from "./reviewModelTypes";

export interface Settings {
  /** With its access mode, this names the connection of the shared default. */
  preferred_provider: string;
  /** Empty means Automatic. A local server uses `local_model` instead. */
  default_model?: string;
  default_effort?: string;
  /** New conversations follow the default only once one has been chosen. */
  default_model_chosen?: boolean;
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
  /** Provider and policy used to build the document overview. */
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
  /** Retired CLI selection retained for saved-settings compatibility. */
  antigravity_cli_model_selection?: ModelSelection;
  antigravity_api_model_selection?: ModelSelection;
  /** Legacy effort retained for saved-settings compatibility. */
  antigravity_effort: string;
  /** "llm" | "auto" | "paddleocr-vl-full" | "pdftotext". */
  pdf_extractor: string;
  /** PaddleOCR-VL page slots; 0 = platform-aware automatic selection. */
  paddle_page_concurrency: number;
  /** llama.cpp multimodal encoder batch size; 0 = automatic. */
  paddle_mtmd_batch_tokens: number;
  /** "auto" | "on" | "off". */
  paddle_flash_attention: string;
  /** Use the optional Windows graphics-card component when installed. */
  paddle_use_gpu: boolean;
  /** Processor cores for layout and built-in recognition: 1, 2, 4, 8, or 16. */
  paddle_cpu_threads: number;
  /** Maximum generated tokens for one OCR page. */
  paddle_max_output_tokens: number;
  /** Retries for a failed or suspicious OCR page. */
  paddle_page_retries: number;
  /** Official full-parser layout and document-restructuring controls. */
  paddle_full_layout_detection: boolean;
  paddle_full_layout_threshold: number;
  /** Persisted one-time migration from the former 0.5 default. */
  paddle_full_layout_threshold_version?: number;
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
  /** Queued reviews started at the same time (1–4). */
  max_concurrent_reviews: number;
  /** Per-Review limits; 0 means no limit. */
  review_max_cost_usd?: number;
  review_max_calls?: number;
  review_max_minutes?: number;
  /** Add an LLM revision reconciliation when a matching prior run exists. */
  auto_revision_reconciliation: boolean;
  /** Max past runs to keep on disk; 0 = keep all. */
  max_saved_runs: number;
  max_saved_run_bytes: number;
  /** Explicit cloud connection mode; Claude subscription uses its CLI. */
  claude_access_mode: "subscription" | "api";
  /** Codex subscription uses App Server by default; CLI is an advanced option. */
  codex_backend?: "legacy_cli" | "app_server";
  /** Persisted one-time migration from the former legacy CLI default. */
  codex_backend_preference_version?: number;
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
