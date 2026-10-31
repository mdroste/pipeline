import { describe, expect, it } from "vitest";
import {
  connections,
  reviewConnectionId,
  reviewConnectionLabel,
} from "./connections";
import type { Settings } from "./types";

describe("connection selection", () => {
  it("keeps saved conversation choices independent of Review defaults", () => {
    const settings = {
      codex_access_mode: "subscription",
      openai_api_key: "test",
    } as Settings;
    expect(reviewConnectionId(settings, "codex")).toBe("codex");
    expect(
      connections.find((connection) => connection.id === "openai-api")?.access,
    ).toBe("api");
    expect(
      reviewConnectionId({ ...settings, codex_access_mode: "api" }, "codex"),
    ).toBe("openai-api");
    const legacy = JSON.parse(
      '{"codex_access_mode":"","openai_api_key":"test"}',
    ) as Settings;
    expect(reviewConnectionId(legacy, "codex")).toBe("openai-api");
    expect(
      reviewConnectionId({ ...legacy, openai_api_key: " " }, "codex"),
    ).toBe("codex");
  });
  it("uses app names for each Review connection", () => {
    expect(reviewConnectionLabel("antigravity", "api")).toBe("Gemini API");
    expect(reviewConnectionLabel("claude", "cli")).toBe("Claude Code");
    expect(reviewConnectionLabel("codex", "api")).toBe("OpenAI API");
  });
});
