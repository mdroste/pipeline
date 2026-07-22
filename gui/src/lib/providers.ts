import type { ModelCatalog, ModelSelection, Settings } from "./types";

export const PROVIDERS = ["claude", "codex", "gemini", "local"] as const;
export type Provider = (typeof PROVIDERS)[number];
export type CloudProvider = Exclude<Provider, "local">;

export function providerTransport(settings: Settings, provider: string): "cli" | "api" {
  if (provider === "local") return "api";
  if (provider === "claude") return settings.anthropic_api_key ? "api" : "cli";
  if (provider === "codex") return settings.openai_api_key ? "api" : "cli";
  return settings.google_api_key ? "api" : "cli";
}

export function providerSelection(
  settings: Settings,
  provider: CloudProvider,
): ModelSelection {
  const transport = providerTransport(settings, provider);
  const selection = provider === "claude"
    ? (transport === "cli" ? settings.claude_cli_model_selection : settings.claude_api_model_selection)
    : provider === "codex"
      ? (transport === "cli" ? settings.codex_cli_model_selection : settings.codex_api_model_selection)
      : (transport === "cli" ? settings.gemini_cli_model_selection : settings.gemini_api_model_selection);
  if (selection) return selection;

  const legacy = provider === "claude"
    ? settings.claude_model
    : provider === "codex"
      ? settings.codex_model
      : settings.gemini_model;
  return legacy ? { mode: "pinned", model: legacy } : { mode: "automatic" };
}

export function withProviderSelection(
  settings: Settings,
  provider: CloudProvider,
  selection: ModelSelection,
): Settings {
  const transport = providerTransport(settings, provider);
  const patch: Partial<Settings> = {};
  if (provider === "claude") {
    if (transport === "cli") patch.claude_cli_model_selection = selection;
    else patch.claude_api_model_selection = selection;
    patch.claude_model = "";
  } else if (provider === "codex") {
    if (transport === "cli") patch.codex_cli_model_selection = selection;
    else patch.codex_api_model_selection = selection;
    patch.codex_model = "";
  } else {
    if (transport === "cli") patch.gemini_cli_model_selection = selection;
    else patch.gemini_api_model_selection = selection;
    patch.gemini_model = "";
  }
  return { ...settings, ...patch };
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

export function decodeModelSelection(value: string): ModelSelection | undefined {
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
  const id = selection?.mode === "pinned"
    ? selection.model
    : selection?.mode === "role"
      ? catalog.roles.find((role) => role.id === selection.role)?.model
      : catalog.default_model || catalog.recommended_model;
  const efforts = catalog.models.find((model) => model.id === id)?.supported_efforts;
  return efforts?.length ? efforts : fallback;
}
