import { describe, expect, it } from "vitest";
import {
  isUnrecordedModelLabel,
  recordedEffortLabel,
  recordedModelLabel,
  unsetEffortLabel,
  unsetEffortOption,
  stageModelDetails,
  unsetModelOption,
  unsetModelSummary,
} from "./modelChoice";
import type { ModelCatalog } from "./reviewModelTypes";
import type { Settings } from "./settingsTypes";

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
      "Opus 5.5 (auto)",
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
      "Sonnet 4.6 (auto)",
    );
  });

  it("uses the shared default model on its own provider only", () => {
    const shared = {
      ...subscription,
      default_model: "gpt-5.6-sol",
      default_effort: "high",
    } as Settings;
    expect(unsetModelOption(shared, "codex")).toBe("gpt-5.6-sol (default)");
    expect(unsetEffortLabel(shared, "codex")).toBe("Default thinking: High");
    expect(unsetModelOption(shared, "claude", claudeCli)).toBe(
      "Opus 5.5 (auto)",
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

describe("live thinking defaults", () => {
  const catalog = {
    provider: "codex",
    transport: "cli",
    default_model: "model-a",
    models: [
      { id: "model-a", display_name: "Model A", default_effort: "medium" },
      { id: "model-b", display_name: "Model B", default_effort: "high" },
      { id: "unknown", display_name: "Unknown", supported_efforts: ["low"] },
    ],
    roles: [{ id: "quality", model: "model-b" }],
  } as ModelCatalog;

  it("shows the live automatic default with the useful value first", () => {
    expect(unsetEffortOption(subscription, "codex", catalog)).toBe(
      "Medium (default)",
    );
    expect(unsetEffortLabel(subscription, "codex", catalog)).toBe(
      "ChatGPT default thinking: Medium",
    );
    expect(unsetModelOption(subscription, "codex", catalog)).toBe(
      "Model A (auto)",
    );
  });

  it("uses the selected model, role, or inherited model rather than always the provider default", () => {
    const pinned = { mode: "pinned", model: "model-b" } as const;
    expect(unsetEffortOption(subscription, "codex", catalog, pinned)).toBe(
      "High (default)",
    );
    expect(
      unsetEffortOption(subscription, "codex", catalog, {
        mode: "role",
        role: "quality",
      }),
    ).toBe("High (default)");
    const shared = { ...subscription, default_model: "model-b" };
    expect(unsetEffortOption(shared, "codex", catalog)).toBe("High (default)");
    expect(
      unsetEffortOption(shared, "codex", catalog, { mode: "automatic" }),
    ).toBe("Medium (default)");
    expect(
      stageModelDetails(
        subscription,
        "codex",
        { "codex:cli": pinned },
        {},
        { codex: catalog },
      ).effort,
    ).toBe("ChatGPT default thinking: High");
  });

  it("keeps a configured thinking override ahead of model defaults", () => {
    const shared = { ...subscription, default_effort: "low" };
    expect(unsetEffortOption(shared, "codex", catalog)).toBe("Low (default)");
    expect(unsetEffortLabel(shared, "codex", catalog)).toBe(
      "Default thinking: Low",
    );
    expect(
      stageModelDetails(
        shared,
        "codex",
        {},
        { "codex:cli": "high" },
        { codex: catalog },
      ).effort,
    ).toBe("High");
  });

  it("does not guess from supported levels or fall back from an unknown pinned model", () => {
    for (const id of ["unknown", "missing"]) {
      expect(
        unsetEffortOption(subscription, "codex", catalog, {
          mode: "pinned",
          model: id,
        }),
      ).toBe("Provider default");
    }
    expect(unsetEffortOption(subscription, "codex")).toBe("Provider default");
    expect(
      unsetEffortOption(subscription, "codex", catalog, {
        mode: "role",
        role: "missing",
      }),
    ).toBe("Provider default");
  });

  it("follows API Automatic's recommended model", () => {
    const api = { ...subscription, codex_access_mode: "api" } as Settings;
    expect(
      unsetEffortOption(api, "codex", {
        ...catalog,
        transport: "api",
        recommended_model: "model-b",
      }),
    ).toBe("High (default)");
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
