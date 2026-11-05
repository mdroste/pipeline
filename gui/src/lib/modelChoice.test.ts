import { describe, expect, it } from "vitest";
import {
  isUnrecordedModelLabel,
  recordedEffortLabel,
  recordedModelLabel,
  unsetEffortLabel,
  unsetModelOption,
  unsetModelSummary,
} from "./modelChoice";
import type { ModelCatalog, Settings } from "./types";

const subscription = {
  preferred_provider: "codex",
  claude_access_mode: "subscription",
  codex_access_mode: "subscription",
  default_model: "",
  default_effort: "",
  local_model: "",
} as unknown as Settings;

const claudeCli = {
  provider: "claude",
  transport: "cli",
  default_model: "claude-opus-5-5",
  models: [{ id: "claude-opus-5-5", display_name: "Opus 5.5" }],
  roles: [],
} as unknown as ModelCatalog;

const anthropicApi = {
  provider: "claude",
  transport: "api",
  default_model: "claude-opus-5-5",
  recommended_model: "claude-sonnet-4-6",
  models: [
    { id: "claude-opus-5-5", display_name: "Opus 5.5" },
    { id: "claude-sonnet-4-6", display_name: "Sonnet 4.6" },
  ],
  roles: [],
} as unknown as ModelCatalog;

describe("unset models", () => {
  it("names the model a CLI picks and who picks it", () => {
    expect(unsetModelOption(subscription, "claude", claudeCli)).toBe(
      "Claude Code's choice: Opus 5.5",
    );
    expect(unsetModelSummary(subscription, "claude", claudeCli)).toBe(
      "Opus 5.5 (Claude Code's choice)",
    );
  });

  it("says who picks while the model list is unknown", () => {
    expect(unsetModelOption(subscription, "claude", undefined, true)).toBe(
      "Claude Code's choice (checking…)",
    );
    expect(unsetModelSummary(subscription, "codex")).toBe("ChatGPT's choice");
  });

  it("names Pipeline's recommended model on an API connection", () => {
    const api = { ...subscription, claude_access_mode: "api" } as Settings;
    expect(unsetModelOption(api, "claude", anthropicApi)).toBe(
      "Pipeline's choice: Sonnet 4.6",
    );
  });

  it("uses the shared default model on its own provider only", () => {
    const shared = {
      ...subscription,
      default_model: "gpt-5.6-sol",
      default_effort: "high",
    } as Settings;
    expect(unsetModelOption(shared, "codex")).toBe(
      "Default model: gpt-5.6-sol",
    );
    expect(unsetEffortLabel(shared, "codex")).toBe("Default thinking: High");
    expect(unsetModelOption(shared, "claude", claudeCli)).toBe(
      "Claude Code's choice: Opus 5.5",
    );
    expect(unsetEffortLabel(shared, "claude")).toBe(
      "Claude Code default thinking",
    );
  });

  it("names the local server's model", () => {
    expect(
      unsetModelOption({ ...subscription, local_model: "qwen3" }, "local"),
    ).toBe("Local server: qwen3");
    expect(unsetModelOption(subscription, "local")).toBe(
      "Local server: no model selected",
    );
  });
});

describe("recorded models", () => {
  it("never shows the placeholder older builds stored", () => {
    const label = recordedModelLabel("claude", "cli", "Provider default");
    expect(label).toBe("Claude Code's choice (not recorded)");
    expect(isUnrecordedModelLabel(label)).toBe(true);
    expect(recordedModelLabel("codex", "cli", "")).toBe(
      "ChatGPT's choice (not recorded)",
    );
  });

  it("marks a model that was expected rather than reported", () => {
    expect(recordedModelLabel("codex", "cli", "gpt-5.6-sol", "expected")).toBe(
      "gpt-5.6-sol (expected)",
    );
    expect(
      recordedModelLabel("claude", "cli", "claude-opus-5-5", "reported"),
    ).toBe("claude-opus-5-5");
  });

  it("names whose default thinking level applied", () => {
    expect(recordedEffortLabel("claude", "cli", "default")).toBe(
      "Claude Code default",
    );
    expect(recordedEffortLabel("codex", "api", "")).toBe("OpenAI API default");
    expect(recordedEffortLabel("codex", "api", "high")).toBe("high");
  });
});
