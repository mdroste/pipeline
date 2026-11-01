import type { Settings } from "../../lib/types";
import type { AppPreferences } from "../../lib/appPreferences";
import type { ThemePreference } from "../../lib/theme";
import type { Section } from "./navigation";
import { connectionDescription } from "../../lib/connections";
import {
  defaultConnectionId,
  defaultOrientationAgent,
  defaultParallelAgents,
  defaultMergeAgent,
  defaultSequentialAgent,
  providerTransport,
  PROVIDER_LABELS,
  type Provider,
} from "../../lib/providers";

export interface SettingsSearchEntry {
  section: Section;
  target: string;
  label: string;
  description: string;
  value: string;
}
export function settingsSearchEntries(
  s: Settings,
  p: AppPreferences,
  theme: ThemePreference,
): SettingsSearchEntry[] {
  const entries: SettingsSearchEntry[] = [];
  const add = (
    section: Section,
    target: string,
    label: string,
    description: string,
    value: unknown,
  ) =>
    entries.push({
      section,
      target,
      label,
      description,
      value:
        typeof value === "boolean"
          ? value
            ? "On"
            : "Off"
          : String(value ?? "Default"),
    });
  add(
    "general",
    "general-preferences",
    "Appearance",
    "Theme, system, light or dark mode",
    theme,
  );
  add(
    "general",
    "interface-scale",
    "Interface text size",
    "Scale, zoom, accessibility",
    `${p.interfaceScale}%`,
  );
  add(
    "general",
    "report-scale",
    "Default report text size",
    "Reading, reports, font, zoom",
    `${p.reportScale}%`,
  );
  add(
    "general",
    "interface-density",
    "Spacing",
    "Comfortable or compact interface density",
    p.density,
  );
  add(
    "general",
    "editor-font",
    "Editor font size",
    "Source files and manuscript editing",
    `${p.editorFontSize} px`,
  );
  add(
    "general",
    "editor-wrap",
    "Wrap long lines",
    "Word wrap in source editors",
    p.editorWrap,
  );
  add(
    "general",
    "startup-page",
    "When Pipeline opens",
    "Startup, restore last project or conversation, Home overview",
    p.startup === "restore" ? "Continue where I left off" : "Home overview",
  );
  add(
    "general",
    "guided-setup",
    "Guided setup",
    "Get started, set up Pipeline, check connections and installed tools, example review",
    "Set up Pipeline",
  );
  add(
    "conversations",
    "send-shortcut",
    "Send message with",
    "Enter, Command, Ctrl, keyboard shortcut",
    p.sendShortcut === "enter" ? "Enter" : "Command/Ctrl+Enter",
  );
  add(
    "conversations",
    "conversation-titles",
    "Conversation titles",
    "Automatically name conversations; title model and thinking level",
    "",
  );
  add(
    "providers",
    "anthropic-provider",
    "Anthropic connection",
    "Claude account, subscription, API key",
    `${s.claude_access_mode} · ${s.anthropic_api_key ? "Key saved" : "No API key"}`,
  );
  add(
    "providers",
    "openai-provider",
    "OpenAI for reviews",
    "ChatGPT account, subscription, API key",
    `${s.codex_access_mode} · ${s.openai_api_key ? "Key saved" : "No API key"}`,
  );
  add(
    "providers",
    "workspace-provider",
    "ChatGPT account",
    "ChatGPT sign-in, account, models, usage limits",
    "",
  );
  add(
    "providers",
    "google-provider",
    "Google connection",
    "Gemini API key",
    s.google_api_key ? "API key saved" : "API key required",
  );
  add(
    "providers",
    "local-provider",
    "Local server",
    "Ollama, LM Studio, llama.cpp, OpenAI-compatible server, local model, endpoint, API key",
    s.local_model || "No model selected",
  );
  // The control exists only with a subscription connection.
  if (s.codex_access_mode !== "api")
    add(
      "providers",
      "Reviews Codex backend",
      "Reviews connection",
      "Codex App Server or legacy Codex CLI, compatibility",
      s.codex_backend || "app_server",
    );
  add(
    "workflow",
    "workflow-models",
    "Model defaults",
    "Input assessment orientation, parallel review, combine merge results, sequential steps, provider, thinking, reasoning",
    "",
  );
  for (const [label, target, agents, models, efforts] of [
    [
      "Input assessment",
      "Input assessment providers",
      [defaultOrientationAgent(s)],
      s.default_orientation_model_overrides,
      s.default_orientation_effort_overrides,
    ],
    [
      "Parallel review",
      "Parallel review providers",
      defaultParallelAgents(s),
      s.default_parallel_model_overrides,
      s.default_parallel_effort_overrides,
    ],
    [
      "Combine results",
      "Combine results providers",
      [defaultMergeAgent(s)],
      s.default_merge_model_overrides,
      s.default_merge_effort_overrides,
    ],
    [
      "Sequential steps",
      "Sequential steps providers",
      [defaultSequentialAgent(s)],
      s.default_sequential_model_overrides,
      s.default_sequential_effort_overrides,
    ],
  ] as const) {
    const value = agents
      .map((agent) => {
        const key = `${agent}:${providerTransport(s, agent)}`;
        const model = models?.[key] ?? models?.[agent];
        return `${PROVIDER_LABELS[agent as Provider]} · ${model?.mode === "pinned" ? model.model : model?.mode === "role" ? model.role : "Provider default"} · ${efforts?.[key] || efforts?.[agent] || "Default thinking"}`;
      })
      .join("; ");
    add(
      "workflow",
      target,
      label,
      "Review stage provider, model, and thinking level",
      value,
    );
  }
  add(
    "workflow",
    "usage-limit-fallback",
    "Continue after an account usage limit",
    "Backup provider, fallback, subscription window, API quota, credits",
    s.usage_limit_fallback_agent || "Off",
  );
  add(
    "providers",
    "default-model",
    "Default model",
    "Preferred provider, model, thinking, effort, reviews and new conversations",
    connectionDescription(defaultConnectionId(s)).label,
  );
  add(
    "workflow",
    "Model calls at once",
    "Model calls at once",
    "Concurrent agents, execution concurrency, parallel workers",
    s.max_workers,
  );
  add(
    "workflow",
    "Step time limit",
    "Step time limit",
    "Timeout, maximum time for a model call",
    `${s.step_timeout_secs / 60} minutes`,
  );
  add(
    "workflow",
    "Step retries",
    "Step retries",
    "Retry failed calls",
    s.max_retries,
  );
  add(
    "workflow",
    "max-concurrent-reviews",
    "Reviews at once",
    "Simultaneous reviews, queue, concurrency, parallel reviews",
    s.max_concurrent_reviews ?? 2,
  );
  add(
    "workflow",
    "review-limits",
    "Review limits",
    "Cost limit, spending cap, budget, maximum calls, time limit, stop a review",
    [
      s.review_max_cost_usd ? `$${s.review_max_cost_usd}` : "",
      s.review_max_calls ? `${s.review_max_calls} calls` : "",
      s.review_max_minutes ? `${s.review_max_minutes} minutes` : "",
    ]
      .filter(Boolean)
      .join(" · ") || "No limits",
  );
  add(
    "workflow",
    "workflow-extraction",
    "PDF extraction",
    "OCR, document reading, extraction method, PaddleOCR, pdftotext",
    s.pdf_extractor,
  );
  add(
    "workflow",
    "PDF extraction time limit",
    "Extraction time limit",
    "Time budget, timeout for reading a complete PDF",
    `${s.pdf_extraction_timeout_secs / 60} minutes`,
  );
  add(
    "workflow",
    "Compare with the previous report",
    "Compare with the previous report",
    "Revision reconciliation, addressed, remaining and new concerns between revisions",
    s.auto_revision_reconciliation,
  );
  add(
    "storage",
    "general-storage",
    "Research data folder",
    "Storage directory, Dropbox, restart, active and pending folder",
    "",
  );
  add(
    "storage",
    "workspace-research-data",
    "Research backups & cleanup",
    "Backup, restore archive, conversations, retention, disk usage, Trash",
    "",
  );
  add(
    "storage",
    "Maximum saved reports",
    "Maximum saved reports",
    "Report history retention, cleanup, Trash",
    s.max_saved_runs || "Unlimited",
  );
  add(
    "storage",
    "Review history size limit in GB",
    "Report history size limit",
    "Retention, disk usage, cleanup",
    s.max_saved_run_bytes ? `${s.max_saved_run_bytes / 1e9} GB` : "Unlimited",
  );
  add(
    "storage",
    "storage-cache",
    "Reuse earlier PDF extractions",
    "Extraction cache, temporary PDF data, avoid repeating extraction",
    s.reuse_pdf_extraction_cache,
  );
  for (const [key, label, description] of [
    [
      "notifyCompletion",
      "Work completed",
      "Reply, review, batch and automation completion notifications",
    ],
    ["notifyFailure", "Failures", "Error notifications"],
    [
      "notifyAttention",
      "Attention required",
      "Approval, input and research check notifications",
    ],
    [
      "suppressFocused",
      "Quiet while I’m using Pipeline",
      "Suppress notifications when focused",
    ],
    ["notificationSound", "Play a sound", "Notification audio tone"],
    [
      "desktopNotifications",
      "Desktop notifications",
      "System notification permission",
    ],
  ] as const)
    add(
      "notifications",
      key === "notificationSound"
        ? "notification-sound"
        : key === "desktopNotifications"
          ? "desktop-notifications"
          : key,
      label,
      description,
      p[key],
    );
  add(
    "advanced",
    "Verbose console logging",
    "Verbose console logging",
    "Diagnostics, commands, subprocess output",
    s.verbose_logging,
  );
  add(
    "advanced",
    "app-version",
    "Version & updates",
    "About Pipeline, check for update, automatic updates, download, release notes",
    "",
  );
  add(
    "advanced",
    "workspace-diagnostics",
    "Installed tools",
    "Diagnostics, capabilities, Git, LaTeX, PDF tools, Stata",
    "",
  );
  const parser = [
    [
      "PaddleOCR-VL concurrent pages",
      "Concurrent pages",
      "paddle_page_concurrency",
    ],
    [
      "PaddleOCR-VL image batch size",
      "Image batch size",
      "paddle_mtmd_batch_tokens",
    ],
    [
      "PaddleOCR-VL maximum page output",
      "Maximum page output",
      "paddle_max_output_tokens",
    ],
    ["PaddleOCR-VL page retries", "Page retries", "paddle_page_retries"],
    [
      "PaddleOCR-VL Flash Attention",
      "Flash Attention",
      "paddle_flash_attention",
    ],
    [
      "Layout detection and reading order",
      "Layout detection and reading order",
      "paddle_full_layout_detection",
    ],
    [
      "PaddleOCR-VL layout confidence threshold",
      "Layout confidence threshold",
      "paddle_full_layout_threshold",
    ],
    [
      "Remove overlapping regions",
      "Remove overlapping regions",
      "paddle_full_layout_nms",
    ],
    [
      "PaddleOCR-VL overlapping layout boxes",
      "Overlapping layout boxes",
      "paddle_full_layout_merge_bboxes_mode",
    ],
    [
      "Merge split regions",
      "Merge split regions",
      "paddle_full_merge_layout_blocks",
    ],
    [
      "Read text inside images",
      "Read text inside images",
      "paddle_full_ocr_image_blocks",
    ],
    [
      "Save regions as Markdown",
      "Save regions as Markdown",
      "paddle_full_format_block_content",
    ],
    [
      "Merge tables across pages",
      "Merge tables across pages",
      "paddle_full_merge_tables",
    ],
    ["Fix heading levels", "Fix heading levels", "paddle_full_relevel_titles"],
    [
      "Keep equation numbers",
      "Keep equation numbers",
      "paddle_full_show_formula_numbers",
    ],
  ] as const;
  // Technical names stay searchable behind the plain labels.
  const parserTerms: Partial<Record<(typeof parser)[number][2], string>> = {
    paddle_mtmd_batch_tokens: ", vision encoder",
    paddle_full_layout_nms: ", NMS, suppress layout detections",
    paddle_full_merge_layout_blocks: ", layout blocks",
    paddle_full_format_block_content: ", format block content",
    paddle_full_relevel_titles: ", relevel titles",
    paddle_full_show_formula_numbers: ", formula numbers",
  };
  for (const [target, label, key] of parser)
    add(
      "workflow",
      target,
      label,
      `Advanced PDF extraction, PaddleOCR parser${parserTerms[key] ?? ""}`,
      s[key],
    );
  return entries;
}

export function searchSettings(entries: SettingsSearchEntry[], query: string) {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return words.length
    ? entries.filter((entry) =>
        words.every((word) =>
          `${entry.label} ${entry.description} ${entry.value}`
            .toLowerCase()
            .includes(word),
        ),
      )
    : [];
}
