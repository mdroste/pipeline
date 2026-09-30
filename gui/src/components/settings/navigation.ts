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
  if (section === "workspace" || section === "api-keys") return "providers";
  if (section === "llm" || section === "extraction") return "workflow";
  return section;
}

export const SECTION_INFO = {
  general: {
    title: "General",
    description: "Make Pipeline comfortable to read and choose where to start.",
    detail: "Appearance & startup",
    icon: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5",
  },
  providers: {
    title: "Connections",
    description: "Manage model connections, accounts, and API keys.",
    detail: "Accounts & connections",
    icon: "M8 3v4m8-4v4M6 7h12v3a6 6 0 0 1-12 0V7Zm6 9v5",
  },
  workflow: {
    title: "Reviews",
    description:
      "Model, execution, and report defaults for Reviews. Individual workflows can override these choices.",
    detail: "Models, execution & history",
    icon: "M6 3v12m0 0a3 3 0 1 0 0 6 3 3 0 0 0 0-6Zm12-12v4m0 0a3 3 0 1 0 0 6 3 3 0 0 0 0-6Zm0 6v8M6 9h9",
  },
  conversations: {
    title: "Conversations",
    description:
      "Message shortcuts and naming preferences for your conversations.",
    detail: "Messages & automatic titles",
    icon: "M4 4h16v12H8l-4 4V4Z",
  },
  storage: {
    title: "Data & Storage",
    description:
      "Research folders, backups, report retention, and temporary data on this device.",
    detail: "Folders, backups & cleanup",
    icon: "M3 7h7l2 2h9v11H3V7Zm0 0V4h7l2 3",
  },
  notifications: {
    title: "Notifications",
    description:
      "Choose when Pipeline lets you know that work is complete or needs attention.",
    detail: "Completion, errors & attention",
    icon: "M18 8a6 6 0 0 0-12 0v6l-2 3h16l-2-3V8ZM10 21h4",
  },
  advanced: {
    title: "Advanced & About",
    description:
      "Diagnostic information, compatibility options, and app updates.",
    detail: "Diagnostics & version",
    icon: "M12 8v4m0 4h.01M3 12a9 9 0 1 0 18 0 9 9 0 0 0-18 0Z",
  },
};

export const PROVIDER_LINKS = [
  { id: "anthropic-provider", label: "Anthropic" },
  { id: "openai-provider", label: "OpenAI" },
  { id: "google-provider", label: "Google" },
  { id: "local-provider", label: "Compatible server" },
];

export const WORKFLOW_LINKS = [
  { id: "workflow-models", label: "Models" },
  { id: "workflow-execution", label: "Execution" },
  { id: "workflow-extraction", label: "PDF Extraction" },
  { id: "workflow-history", label: "Reports & history" },
];

// Every navigable target now lives in the section its caller declares (the
// search index and jump links name their sections explicitly), so resolving
// a destination needs no per-target exception table.
export function sectionForTarget(
  section: SettingsSection,
  _target?: string,
): Section {
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
