export type Section =
  | "general"
  | "providers"
  | "conversations"
  | "workflow"
  | "storage"
  | "notifications"
  | "advanced";
// Keep existing entry points (dependency repair and Workspace sign-in) working.
export type SettingsSection =
  Section | "workspace" | "llm" | "api-keys" | "extraction";

export function resolveSection(section: SettingsSection): Section {
  if (section === "workspace" || section === "api-keys" || section === "llm")
    return "providers";
  if (section === "extraction") return "workflow";
  return section;
}

export const SECTION_INFO: Record<
  Section,
  { title: string; description?: string }
> = {
  general: {
    title: "General",
  },
  providers: {
    title: "Models",
    description:
      "Choose default models and connect Claude, ChatGPT, Gemini, or a local server.",
  },
  workflow: {
    title: "Reviews",
    description:
      "Execution limits and PDF extraction for reviews. A review design can override these.",
  },
  conversations: {
    title: "Conversations",
  },
  storage: {
    title: "Data & Storage",
    description: "Where your research is saved, plus backups and cleanup.",
  },
  notifications: {
    title: "Notifications",
  },
  advanced: {
    title: "Advanced & About",
    description: "Logging, installed tools, and app updates.",
  },
};

export const PROVIDER_LINKS = [
  { id: "default-model", label: "Default model" },
  { id: "workflow-models", label: "Review models" },
  { id: "anthropic-provider", label: "Anthropic" },
  { id: "openai-provider", label: "OpenAI" },
  { id: "google-provider", label: "Google" },
  { id: "local-provider", label: "Local server" },
];

export const WORKFLOW_LINKS = [
  { id: "workflow-execution", label: "Execution" },
  { id: "workflow-extraction", label: "PDF extraction" },
  { id: "workflow-history", label: "Reports & history" },
];

// Preserve links made before review model preferences moved to Models.
export function sectionForTarget(
  section: SettingsSection,
  target?: string,
): Section {
  if (
    target === "workflow-models" ||
    target === "usage-limit-fallback" ||
    target === "Document bundle preparation providers" ||
    target === "Parallel steps providers" ||
    target === "Combine results providers" ||
    target === "Sequential steps providers"
  )
    return "providers";
  return resolveSection(section);
}

export function focusSettingsTarget(id: string) {
  const target =
    document.getElementById(id) ??
    Array.from(document.querySelectorAll<HTMLElement>("[aria-label]")).find(
      (element) => element.getAttribute("aria-label") === id,
    );
  if (!target) return;
  let ancestor: HTMLElement | null = target;
  while (ancestor) {
    if (ancestor instanceof HTMLDetailsElement) ancestor.open = true;
    ancestor = ancestor.parentElement;
  }
  const control = target.matches("input,select,button,textarea")
    ? target
    : target.querySelector<HTMLElement>(
        "input:not([disabled]),select:not([disabled]),button:not([disabled]),summary",
      );
  (control ?? target).focus({ preventScroll: true });
  target.scrollIntoView?.({ block: "center" });
}
