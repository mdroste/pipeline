/** Connection names and saved IDs are shared with the native app. */
import type { DepsReport, Settings } from "./types";
import registry from "../../connections.json";

export type ConnectionId =
  "codex" | "claude" | "openai-api" | "anthropic-api" | "google-api" | "local";
export type ProviderFamily = "claude" | "codex" | "antigravity" | "local";
export interface ConnectionDescription {
  id: ConnectionId;
  label: string;
  family: ProviderFamily;
  access: "subscription" | "api";
}
export const connections = registry.connections as ConnectionDescription[];
export const providerFamilies = registry.families as {
  id: ProviderFamily;
  label: string;
}[];
export function connectionDescription(id: ConnectionId): ConnectionDescription {
  return connections.find((connection) => connection.id === id)!;
}

export function reviewConnectionId(
  settings: Settings,
  provider: string,
): ConnectionId {
  const family = provider || "claude";
  if (family === "local") return "local";
  if (family === "antigravity") return "google-api";
  const mode =
    family === "codex"
      ? settings.codex_access_mode
      : settings.claude_access_mode;
  const key =
    family === "codex" ? settings.openai_api_key : settings.anthropic_api_key;
  const api = mode === "api" || (!mode && !!key?.trim());
  return (
    connections.find(
      (connection) =>
        connection.family === family &&
        connection.access === (api ? "api" : "subscription"),
    )?.id ?? "claude"
  );
}

export type ConnectionAvailability = "ready" | "missing" | "unknown";

/**
 * Whether a connection looks usable. Readiness last checked `report`, so a
 * signed-out result can be out of date: callers should label a missing
 * connection rather than stop someone from choosing it.
 */
export function connectionAvailability(
  id: ConnectionId,
  settings: Settings,
  report: DepsReport | null | undefined,
): ConnectionAvailability {
  const key = (value: string | undefined) =>
    value?.trim() ? "ready" : "missing";
  if (id === "openai-api") return key(settings.openai_api_key);
  if (id === "anthropic-api") return key(settings.anthropic_api_key);
  if (id === "google-api") return key(settings.google_api_key);
  if (!report) return "unknown";
  const dependency = (...names: string[]) =>
    report.deps.find(({ name }) => names.includes(name));
  if (id === "local")
    return dependency("Local LLM server")?.found ? "ready" : "missing";
  if (id === "claude") {
    const cli = dependency("Claude CLI");
    if (!cli) return "unknown";
    if (cli.cli_auth_status === "signed_in") return "ready";
    if (cli.cli_auth_status === "signed_out") return "missing";
    if (cli.cli_auth_status === "unknown") return "unknown";
    // In API mode `found` describes the key, and no status means no CLI.
    return settings.claude_access_mode !== "api" && cli.found
      ? "ready"
      : "missing";
  }
  // In API mode the report describes the key, not the ChatGPT account.
  if (settings.codex_access_mode === "api") return "unknown";
  const chatgpt = dependency("ChatGPT (reviews)", "Codex CLI");
  if (!chatgpt) return "unknown";
  return chatgpt.found && chatgpt.authenticated !== false ? "ready" : "missing";
}

/** The connection Reviews would use for a provider. */
export function providerAvailability(
  provider: string,
  settings: Settings,
  report: DepsReport | null | undefined,
): ConnectionAvailability {
  // Undefined means this caller has no readiness information at all.
  return report === undefined
    ? "unknown"
    : connectionAvailability(
        reviewConnectionId(settings, provider),
        settings,
        report,
      );
}

export function reviewConnectionLabel(
  provider: string,
  transport: string,
): string {
  return (
    connections.find(
      (connection) =>
        connection.family === provider &&
        connection.access === (transport === "api" ? "api" : "subscription"),
    )?.label ?? provider
  );
}
