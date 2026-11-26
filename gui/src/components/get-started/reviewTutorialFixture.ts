import type { Settings } from "../../lib/settingsTypes";
import type { ModelCatalog, RosterEntry } from "../../lib/reviewModelTypes";
import type { PipelineConfig } from "../../lib/workflowTypes";
import type { createReviewSetupReads } from "../reviewSetupReads";
import type { RunPreviewPlan } from "../RunPreview";
import type { ReviewSetupEnvironmentValue } from "../reviewSetupEnvironment";
import {
  EXAMPLE_REPORT,
  EXAMPLE_CORE_REVIEWERS,
  EXAMPLE_ADAPTIVE_REVIEWERS,
} from "./exampleReviewData";

export const SAMPLE_PATH = "Example/port-harlan-brt.pdf";
export const SAMPLE_NOTE =
  "Focus on identification assumptions, the event-study evidence, and whether the cost-benefit claims follow from the results.";
export const SAMPLE_CONFIG: PipelineConfig = {
  use_orientation: true,
  orientation_prompt: "",
  orientation_schema: {
    "x-pipeline-contract": "auto-review-v2",
    "x-pipeline-adaptive-agent-count": EXAMPLE_ADAPTIVE_REVIEWERS.length,
  },
  extraction: { method: "pdftotext", input_mode: "document" },
  parallel_context_template: "",
  merge: { enabled: false, prompt: "", agents: [] },
  steps: EXAMPLE_REPORT.step_outputs
    .filter(
      (step) => !EXAMPLE_ADAPTIVE_REVIEWERS.some(([id]) => id === step.step_id),
    )
    .map((step) => ({
      id: step.step_id,
      label: step.step_label,
      prompt: "Example review instructions",
      enabled: true,
      phase: step.phase === "parallel" ? "parallel" : "sequential",
      agents: ["claude"],
      tools: [],
      context: {
        include: [{ kind: "primary", parts: ["text"] }, { kind: "survey" }],
      },
    })),
};
const settings: Settings = {
  preferred_provider: "claude",
  default_model: "example-model",
  default_effort: "high",
  default_parallel_agents: [],
  default_parallel_model_overrides: {},
  default_parallel_effort_overrides: {},
  claude_access_mode: "subscription",
  codex_access_mode: "subscription",
  local_model: "",
  anthropic_api_key: "",
  openai_api_key: "",
  google_api_key: "",
  local_base_url: "",
  max_workers: 6,
  active_profile: "auto-paper-review",
  claude_model: "example-model",
  claude_effort: "high",
  codex_model: "example-model",
  codex_effort: "high",
  antigravity_effort: "high",
  pdf_extractor: "pdftotext",
  paddle_page_concurrency: 0,
  paddle_mtmd_batch_tokens: 0,
  paddle_flash_attention: "auto",
  paddle_use_gpu: false,
  paddle_cpu_threads: 2,
  paddle_max_output_tokens: 4096,
  paddle_page_retries: 0,
  paddle_full_layout_detection: false,
  paddle_full_layout_threshold: 0.5,
  paddle_full_layout_nms: false,
  paddle_full_layout_merge_bboxes_mode: "union",
  paddle_full_merge_layout_blocks: false,
  paddle_full_ocr_image_blocks: false,
  paddle_full_format_block_content: false,
  paddle_full_merge_tables: false,
  paddle_full_relevel_titles: false,
  paddle_full_show_formula_numbers: false,
  pdf_extraction_timeout_secs: 60,
  reuse_pdf_extraction_cache: false,
  verbose_logging: false,
  step_timeout_secs: 300,
  max_retries: 0,
  max_concurrent_reviews: 1,
  auto_revision_reconciliation: false,
  max_saved_runs: 0,
  max_saved_run_bytes: 0,
  antigravity_access_mode: "api",
  local_api_key: "",
};

export const SAMPLE_ROSTER: RosterEntry[] = [
  {
    stage: "parallel",
    label: "Core reviewers",
    step_ids: EXAMPLE_CORE_REVIEWERS.map(([id]) => id),
    step_labels: EXAMPLE_CORE_REVIEWERS.map(([, label]) => label),
    provider: "claude",
    transport: "cli",
    model: { mode: "pinned", model: "example-model" },
    effort: "high",
    source: "default",
    note: null,
  },
  {
    stage: "adaptive",
    label: "Tailored adaptive reviewers",
    step_ids: EXAMPLE_ADAPTIVE_REVIEWERS.map(([id]) => id),
    step_labels: EXAMPLE_ADAPTIVE_REVIEWERS.map(([, label]) => label),
    provider: "claude",
    transport: "cli",
    model: { mode: "pinned", model: "example-model" },
    effort: "high",
    source: "default",
    note: "Tailored adaptive reviewers for this paper’s subject and methods.",
  },
  ...SAMPLE_CONFIG.steps
    .filter((step) => step.phase === "sequential")
    .map((step): RosterEntry => ({
      stage: "sequential",
      label: step.label,
      step_ids: [step.id],
      step_labels: [step.label],
      provider: "claude",
      transport: "cli",
      model: { mode: "pinned", model: "example-model" },
      effort: "high",
      source: "default",
      note: null,
    })),
];
export const SAMPLE_READS: ReturnType<typeof createReviewSetupReads> = {
  config: async () => SAMPLE_CONFIG,
  workflow: async () => [
    SAMPLE_CONFIG,
    [
      {
        id: "auto-paper-review",
        name: "Paper Review",
        builtin: true,
        step_count: SAMPLE_CONFIG.steps.length,
      },
    ],
    "auto-paper-review",
  ],
};
export const SAMPLE_ENVIRONMENT: ReviewSetupEnvironmentValue = {
  loadSettings: async () => ({ settings, warnings: [] }),
  modelCatalog: async ({ provider }): Promise<ModelCatalog> => ({
    provider,
    transport: provider === "antigravity" ? "api" : "cli",
    source: "tutorial",
    source_version: "1",
    fetched_at: "",
    stale: false,
    default_model: "example-model",
    recommended_model: "example-model",
    roles: [],
    models: [
      {
        id: "example-model",
        display_name: "Example model",
        description: "Simulated model for this tutorial",
        is_default: true,
        deprecated: false,
        capabilities: [],
        supported_efforts: ["low", "medium", "high"],
      },
    ],
  }),
  modelRoster: async () => SAMPLE_ROSTER,
  pdfOptions: async () => ({ method: "pdftotext" }),
  switchWorkflow: async () => SAMPLE_CONFIG,
  chooseFiles: (async () => [
    SAMPLE_PATH,
  ]) as ReviewSetupEnvironmentValue["chooseFiles"],
  inputFiles: async () => [],
  onFileDrop: async () => () => {},
};
export const SAMPLE_PLAN: RunPreviewPlan = {
  profileId: "auto-paper-review",
  inputMode: "document",
  inputInterpretation: "document",
  providers: ["claude"],
  orientationAgent: "claude",
  modelRoster: SAMPLE_ROSTER,
  stages: [
    {
      id: "prepare",
      kind: "extracting",
      label: "Creating document bundle",
      stepIds: [],
    },
    {
      id: "orient",
      kind: "orienting",
      label: "Identifying content",
      stepIds: [],
    },
    {
      id: "review",
      kind: "dispatching",
      label: "Reviewers working together",
      stepIds: EXAMPLE_REPORT.step_outputs
        .slice(0, -2)
        .map((step) => step.step_id),
      stepLabels: EXAMPLE_REPORT.step_outputs
        .slice(0, -2)
        .map((step) => step.step_label),
    },
    {
      id: "validate",
      kind: "synthesizing",
      label: "Consolidate and validate feedback",
      stepIds: ["auto_synthesis", "auto_validate"],
      stepLabels: ["Consolidate Feedback", "Validate Feedback"],
    },
    { id: "done", kind: "done", label: "Complete", stepIds: [] },
  ],
};
