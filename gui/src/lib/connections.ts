/** Connection names and saved IDs are shared with the native app. */
import type { Settings } from "./types";
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
