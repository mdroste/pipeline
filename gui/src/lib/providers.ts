import type { ModelCatalog, ModelSelection, Settings } from "./types";

export const PROVIDERS = ["claude", "codex", "antigravity", "local"] as const;
export type Provider = (typeof PROVIDERS)[number];

export const PROVIDER_LABELS: Record<Provider, string> = {
  claude: "Claude",
  codex: "ChatGPT",
  antigravity: "Gemini",
  local: "Local",
};

export function defaultParallelAgents(settings: Settings): string[] {
  return settings.default_parallel_agents?.length
    ? settings.default_parallel_agents
    : [settings.preferred_provider || "claude"];
}

export function defaultSequentialAgent(settings: Settings): string {
  return (
    settings.default_sequential_agent || settings.preferred_provider || "claude"
  );
}

export function defaultMergeAgent(settings: Settings): string {
  return settings.default_merge_agent || defaultSequentialAgent(settings);
}

export function defaultMergeModelOverrides(
  settings: Settings,
): Record<string, ModelSelection> {
  return settings.default_merge_agent
    ? (settings.default_merge_model_overrides ?? {})
    : (settings.default_sequential_model_overrides ?? {});
}

export function defaultMergeEffortOverrides(
  settings: Settings,
): Record<string, string> {
  return settings.default_merge_agent
    ? (settings.default_merge_effort_overrides ?? {})
    : (settings.default_sequential_effort_overrides ?? {});
}

export function defaultOrientationAgent(settings: Settings): string {
  return (
    settings.default_orientation_agent ||
    settings.preferred_provider ||
    "claude"
  );
}

export function providerTransport(
  settings: Settings,
  provider: string,
): "cli" | "api" {
  if (provider === "local") return "api";
  if (provider === "codex") {
    if (settings.codex_access_mode)
      return settings.codex_access_mode === "api" ? "api" : "cli";
    return settings.openai_api_key ? "api" : "cli";
  }
  // Google subscription dispatch (the agy CLI) is disabled; mirrors
  // Settings::model_transport in the backend.
  if (provider === "antigravity") return "api";
  if (settings.claude_access_mode)
    return settings.claude_access_mode === "api" ? "api" : "cli";
  return settings.anthropic_api_key ? "api" : "cli";
}

export function encodeModelSelection(
  selection: ModelSelection | undefined,
  emptyValue = "inherit",
): string {
  if (!selection) return emptyValue;
  if (selection.mode === "automatic") return "automatic";
  if (selection.mode === "role") return `role:${selection.role}`;
  return `pinned:${selection.model}`;
}

export function decodeModelSelection(
  value: string,
): ModelSelection | undefined {
  if (value === "inherit") return undefined;
  if (value === "automatic") return { mode: "automatic" };
  if (value.startsWith("role:")) return { mode: "role", role: value.slice(5) };
  return { mode: "pinned", model: value.slice(7) };
}

export function effortOptions(
  catalog: ModelCatalog | undefined,
  selection: ModelSelection | undefined,
  fallback: string[],
): string[] {
  if (!catalog) return fallback;
  const id =
    selection?.mode === "pinned"
      ? selection.model
      : selection?.mode === "role"
        ? catalog.roles.find((role) => role.id === selection.role)?.model
        : catalog.default_model || catalog.recommended_model;
  const efforts = catalog.models.find(
    (model) => model.id === id,
  )?.supported_efforts;
  return efforts?.length ? efforts : fallback;
}
