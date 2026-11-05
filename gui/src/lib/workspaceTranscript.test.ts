import { describe, expect, it } from "vitest";
import { isMessage, payloadText, roleFor } from "./workspaceTranscript";
import type { TranscriptItem } from "./workbenchTypes";

function item(
  payload: TranscriptItem["payload"],
  itemKind = "agentMessage",
): TranscriptItem {
  return {
    id: "item",
    turnId: null,
    providerItemId: "provider-item",
    itemKind,
    payload,
    isFinal: true,
    createdAt: "2026-10-29T00:00:00Z",
    updatedAt: "2026-10-29T00:00:00Z",
  };
}

describe("transcript presentation", () => {
  it.each([
    [null, ""],
    [{ text: "first", message: "second", content: "third" }, "first"],
    [{ text: "", message: "fallback" }, ""],
    [
      {
        text: ["first", { text: "second" }, null, { text: 3 }, ""],
        message: "fallback",
      },
      "first\nsecond",
    ],
    [{ text: [{ other: "ignored" }], message: "fallback" }, "fallback"],
    [{ content: ["last", { text: "field" }] }, "last\nfield"],
  ] as const)("preserves payload precedence for %j", (payload, expected) => {
    expect(payloadText(item(payload))).toBe(expected);
  });

  it("classifies message kinds without treating tool items as messages", () => {
    expect(isMessage(item(null, "UserMessage"))).toBe(true);
    expect(isMessage(item(null, "agentMESSAGE"))).toBe(true);
    expect(isMessage(item(null, "functionCall"))).toBe(false);
  });

  it("keeps user identity ahead of the recorded assistant runtime", () => {
    expect(roleFor(item({ runtime: "claude" }, "USERMessage"))).toBe("You");
    expect(roleFor(item({ runtime: "claude" }))).toBe("Claude");
    expect(roleFor(item({ runtime: "codex" }))).toBe("ChatGPT");
    expect(roleFor(item(null))).toBe("ChatGPT");
  });
});
