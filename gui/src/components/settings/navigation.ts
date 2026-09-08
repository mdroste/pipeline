export type Section = "general" | "providers" | "workflow";
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
    description:
      "Appearance, research storage, and diagnostic preferences for Pipeline.",
    detail: "Appearance, storage & diagnostics",
    icon: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5",
  },
  providers: {
    title: "Providers",
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

export function focusSettingsTarget(id: string) {
  const target = document.getElementById(id);
  if (target instanceof HTMLDetailsElement) target.open = true;
  target?.focus({ preventScroll: true });
  target?.scrollIntoView?.({ block: "start" });
}
