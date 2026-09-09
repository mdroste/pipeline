import type { Settings, EngineStatus, ModelCatalog } from "../lib/types";
export function makeSettings(): Settings {
  return {
    preferred_provider: "claude",
    max_workers: 16,
    active_profile: "deep-review",
    claude_model: "",
    claude_effort: "",
    codex_model: "",
    codex_effort: "",
    antigravity_effort: "",
    pdf_extractor: "llm",
    paddle_page_concurrency: 0,
    paddle_mtmd_batch_tokens: 0,
    paddle_flash_attention: "auto",
    paddle_max_output_tokens: 4096,
    paddle_page_retries: 1,
    paddle_full_layout_detection: true,
    paddle_full_layout_threshold: 0.5,
    paddle_full_layout_nms: true,
    paddle_full_layout_merge_bboxes_mode: "large",
    paddle_full_merge_layout_blocks: true,
    paddle_full_ocr_image_blocks: true,
    paddle_full_format_block_content: true,
    paddle_full_merge_tables: true,
    paddle_full_relevel_titles: true,
    paddle_full_show_formula_numbers: true,
    pdf_extraction_timeout_secs: 1800,
    reuse_pdf_extraction_cache: true,
    verbose_logging: false,
    step_timeout_secs: 1200,
    max_retries: 1,
    auto_revision_reconciliation: false,
    max_saved_runs: 0,
    max_saved_run_bytes: 5_000_000_000,
    claude_access_mode: "subscription",
    codex_access_mode: "subscription",
    antigravity_access_mode: "subscription",
    anthropic_api_key: "",
    openai_api_key: "",
    google_api_key: "",
    local_base_url: "http://localhost:11434/v1",
    local_model: "",
    local_api_key: "",
  };
}

export function paddleEngine(
  overrides: Partial<EngineStatus> = {},
): EngineStatus {
  return {
    id: "paddleocr-vl-parser",
    label: "PaddleOCR-VL 1.6 Full Parser",
    description: "Layout-aware local extraction.",
    installed: false,
    version: "",
    entry_path: "",
    system_path: "",
    est_download_mb: 2900,
    est_disk_mb: 3800,
    managed_stack_mb: 0,
    installing: false,
    install_progress: null,
    ...overrides,
  };
}

export function catalog(
  provider: string,
  transport: "cli" | "api",
  sourceVersion: string,
): ModelCatalog {
  return {
    provider,
    transport,
    source: "test",
    source_version: sourceVersion,
    fetched_at: "2026-07-27T00:00:00Z",
    stale: false,
    models: [],
    roles: [],
  };
}
