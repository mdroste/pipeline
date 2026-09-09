import type { Settings } from "../../lib/types";
import type { AppPreferences } from "../../lib/appPreferences";
import type { ThemePreference } from "../../lib/theme";
import type { Section } from "./navigation";
import {
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
    "Automatically name conversations; title model and reasoning effort",
    "Open to view",
  );
  add(
    "providers",
    "anthropic-provider",
    "Anthropic connection",
    "Claude account, subscription, API key",
    `${s.claude_access_mode} · ${s.anthropic_api_key ? "Key configured" : "No API key"}`,
  );
  add(
    "providers",
    "openai-provider",
    "OpenAI Review connection",
    "ChatGPT account, subscription, API key",
    `${s.codex_access_mode} · ${s.openai_api_key ? "Key configured" : "No API key"}`,
  );
  add(
    "providers",
    "workspace-provider",
    "Conversation connection",
    "ChatGPT sign-in, account, models, usage limits",
    "Separate sign-in",
  );
  add(
    "providers",
    "google-provider",
    "Google connection",
    "Gemini API key",
    s.google_api_key ? "API key configured" : "API key required",
  );
  add(
    "providers",
    "local-provider",
    "Compatible server",
    "Ollama, LM Studio, llama.cpp, local model, endpoint, API key",
    s.local_model || "No model selected",
  );
  add(
    "providers",
    "Reviews Codex backend",
    "Reviews connection compatibility",
    "Advanced legacy CLI backend",
    s.codex_backend || "app_server",
  );
  add(
    "workflow",
    "workflow-models",
    "Model defaults",
    "Input assessment orientation, parallel review, combine merge results, sequential steps, provider, thinking, reasoning",
    "Expand a stage to view",
  );
  for (const [label, target, agents, models, efforts] of [
    [
      "Input assessment",
      "Orientation map providers",
      [defaultOrientationAgent(s)],
      s.default_orientation_model_overrides,
      s.default_orientation_effort_overrides,
    ],
    [
      "Parallel review",
      "Parallel steps providers",
      defaultParallelAgents(s),
      s.default_parallel_model_overrides,
      s.default_parallel_effort_overrides,
    ],
    [
      "Combine results",
      "Merge providers",
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
        return `${PROVIDER_LABELS[agent as Provider]} · ${model?.mode === "pinned" ? model.model : model?.mode === "role" ? model.role : "Provider default"} · ${efforts?.[key] || efforts?.[agent] || "Default reasoning"}`;
      })
      .join("; ");
    add(
      "workflow",
      target,
      label,
      "Review stage provider, model, thinking and reasoning effort",
      value,
    );
  }
  add(
    "workflow",
    "usage-limit-fallback",
    "Continue after an account usage limit",
    "Fallback agent, subscription window, API quota, credits",
    s.usage_limit_fallback_agent || "Off",
  );
  add(
    "workflow",
    "Preferred Provider",
    "Preferred Provider",
    "Fallback for unassigned providers, extraction and revision comparison",
    s.preferred_provider,
  );
  add(
    "workflow",
    "Maximum Concurrent Agents",
    "Maximum Concurrent Agents",
    "Execution concurrency, parallel workers",
    s.max_workers,
  );
  add(
    "workflow",
    "Step Timeout",
    "Step Timeout",
    "Maximum time for a model call",
    `${s.step_timeout_secs / 60} minutes`,
  );
  add(
    "workflow",
    "Step Retries",
    "Step Retries",
    "Retry failed calls",
    s.max_retries,
  );
  add(
    "workflow",
    "workflow-extraction",
    "PDF Extraction",
    "OCR, document reading, extraction method, PaddleOCR, pdftotext",
    s.pdf_extractor,
  );
  add(
    "workflow",
    "PDF extraction time budget",
    "Extraction time budget",
    "Timeout for reading a complete PDF",
    `${s.pdf_extraction_timeout_secs / 60} minutes`,
  );
  add(
    "workflow",
    "Automatic revision reconciliation",
    "Automatic revision reconciliation",
    "Compare addressed, remaining and new concerns between revisions",
    s.auto_revision_reconciliation,
  );
  add(
    "storage",
    "general-storage",
    "Research data folder",
    "Storage directory, Dropbox, restart, active and pending folder",
    "Open to view location",
  );
  add(
    "storage",
    "workspace-research-data",
    "Research backups & cleanup",
    "Backup, restore archive, conversations, retention, disk usage, Trash",
    "Inspect research data",
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
    "Report history size limit in GB",
    "Report history size limit",
    "Retention, disk usage, cleanup",
    s.max_saved_run_bytes ? `${s.max_saved_run_bytes / 1e9} GB` : "Unlimited",
  );
  add(
    "storage",
    "storage-cache",
    "Reuse verified extraction cache",
    "Temporary PDF data, avoid repeating extraction",
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
    "About Pipeline, check for update, release notes",
    "Check installed version",
  );
  add(
    "advanced",
    "workspace-diagnostics",
    "Research diagnostics",
    "Capabilities, evaluation, performance, Git, LaTeX, PDF tools",
    "Inspect diagnostics",
  );
  const parser = [
    [
      "PaddleOCR-VL concurrent pages",
      "Concurrent pages",
      "paddle_page_concurrency",
    ],
    [
      "PaddleOCR-VL vision encoder batch",
      "Vision encoder batch",
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
      "Layout NMS",
      "Suppress overlapping layout detections",
      "paddle_full_layout_nms",
    ],
    [
      "PaddleOCR-VL overlapping layout boxes",
      "Overlapping layout boxes",
      "paddle_full_layout_merge_bboxes_mode",
    ],
    [
      "Merge layout blocks",
      "Merge layout blocks",
      "paddle_full_merge_layout_blocks",
    ],
    [
      "OCR text inside images",
      "OCR text inside images",
      "paddle_full_ocr_image_blocks",
    ],
    [
      "Format block content",
      "Format block content",
      "paddle_full_format_block_content",
    ],
    [
      "Merge tables across pages",
      "Merge tables across pages",
      "paddle_full_merge_tables",
    ],
    ["Relevel titles", "Relevel titles", "paddle_full_relevel_titles"],
    [
      "Retain formula numbers",
      "Retain formula numbers",
      "paddle_full_show_formula_numbers",
    ],
  ] as const;
  for (const [target, label, key] of parser)
    add(
      "workflow",
      target,
      label,
      "Advanced PDF extraction, PaddleOCR parser",
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
