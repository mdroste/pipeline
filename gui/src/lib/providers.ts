import type { ModelCatalog, ModelSelection, Settings } from "./types";

import {
  providerFamilies,
  connectionDescription,
  reviewConnectionId,
  type ConnectionId,
  type ProviderFamily,
} from "./connections";
export const PROVIDERS = providerFamilies.map(({ id }) => id);
export type Provider = ProviderFamily;
export const PROVIDER_LABELS = Object.fromEntries(
  providerFamilies.map(({ id, label }) => [id, label]),
) as Record<Provider, string>;

/** The connection the shared default points at. */
export function defaultConnectionId(settings: Settings): ConnectionId {
  return reviewConnectionId(settings, settings.preferred_provider);
}

/** A model and thinking level belong to one connection, so changing it clears them. */
export function withDefaultConnection(
  settings: Settings,
  id: ConnectionId,
): Settings {
  const { family, access } = connectionDescription(id);
  return {
    ...settings,
    preferred_provider: family,
    ...(family === "claude" ? { claude_access_mode: access } : {}),
    ...(family === "codex" ? { codex_access_mode: access } : {}),
    default_model: "",
    default_effort: "",
    default_model_chosen: true,
  };
}

/** Changing the default provider's access mode changes the default's connection. */
export function withAccessMode(
  settings: Settings,
  family: "claude" | "codex",
  mode: Settings["claude_access_mode"],
): Settings {
  const key = family === "claude" ? "claude_access_mode" : "codex_access_mode";
  return {
    ...settings,
    [key]: mode,
    ...(settings.preferred_provider === family && settings[key] !== mode
      ? { default_model: "", default_effort: "" }
      : {}),
  };
}

function modelName(catalog: ModelCatalog | undefined, id: string): string {
  return catalog?.models.find((model) => model.id === id)?.display_name || id;
}

export function defaultModelSummary(
  settings: Settings,
  catalogs: Record<string, ModelCatalog>,
): string {
  const provider = settings.preferred_provider;
  const model =
    provider === "local" ? settings.local_model : settings.default_model;
  const effort = settings.default_effort;
  return [
    connectionDescription(defaultConnectionId(settings)).label,
    model ? modelName(catalogs[provider], model) : "Automatic",
    effort ? effort.charAt(0).toUpperCase() + effort.slice(1) : null,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** What a stage or step runs when it picks a provider but no model. */
export function inheritedModelLabel(
  settings: Settings,
  provider: string,
  catalog: ModelCatalog | undefined,
): string {
  if (provider === "local")
    return `Provider default · ${settings.local_model || "not selected"}`;
  return provider === settings.preferred_provider && settings.default_model
    ? `Default · ${modelName(catalog, settings.default_model)}`
    : "Provider default";
}

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
  return connectionDescription(reviewConnectionId(settings, provider))
    .access === "api"
    ? "api"
    : "cli";
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
