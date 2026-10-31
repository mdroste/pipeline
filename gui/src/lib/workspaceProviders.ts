/** Saved conversation connection IDs; independent of Review access-mode settings. */
import {
  connections,
  connectionDescription,
  type ConnectionId,
} from "./connections";
export const conversationProviders = connections.map(({ id, label }) => ({
  value: id,
  label,
}));
export type ConversationProvider = ConnectionId;
export function conversationProvider(value: unknown): ConversationProvider {
  return conversationProviders.find((p) => p.value === value)?.value ?? "codex";
}
export function providerName(value: unknown): string {
  return (
    conversationProviders.find((p) => p.value === value)?.label ?? "ChatGPT"
  );
}
export function isApiProvider(value: ConversationProvider): boolean {
  return connectionDescription(value).access === "api";
}
