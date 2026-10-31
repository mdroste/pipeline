import { providerName } from "./workspaceProviders";
import type { TranscriptItem } from "./workbenchTypes";

export function payloadText(item: TranscriptItem): string {
  const payload = item.payload;
  if (!payload) return "";
  for (const key of ["text", "message", "content"]) {
    const value = payload[key];
    if (typeof value === "string") return value;
    if (Array.isArray(value)) {
      const joined = value
        .map((part) => {
          if (typeof part === "string") return part;
          if (
            part &&
            typeof part === "object" &&
            "text" in part &&
            typeof part.text === "string"
          )
            return part.text;
          return "";
        })
        .filter(Boolean)
        .join("\n");
      if (joined) return joined;
    }
  }
  return "";
}

export function isMessage(item: TranscriptItem) {
  return item.itemKind.toLowerCase().includes("message");
}

export function roleFor(item: TranscriptItem): string {
  return item.itemKind.toLowerCase().includes("user")
    ? "You"
    : item.payload?.runtime === "claude"
      ? "Claude"
      : providerName(item.payload?.runtime);
}
