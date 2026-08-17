import { describe, expect, it } from "vitest";
import type { ModelCatalog, Settings } from "./types";
import {
  decodeModelSelection,
  defaultMergeAgent,
  defaultMergeEffortOverrides,
  defaultMergeModelOverrides,
  effortOptions,
  encodeModelSelection,
  providerTransport,
} from "./providers";

function settings(overrides: Partial<Settings> = {}): Settings {
  return {
    claude_access_mode: "subscription",
    codex_access_mode: "subscription",
    antigravity_access_mode: "subscription",
    anthropic_api_key: "",
    openai_api_key: "",
    google_api_key: "",
    claude_model: "",
    codex_model: "",
    antigravity_effort: "",
    ...overrides,
  } as Settings;
}

describe("provider model utilities", () => {
  it("keeps transport mode explicit even when a key is stored", () => {
    const cli = settings();
    expect(providerTransport(cli, "claude")).toBe("cli");

    const storedKey = { ...cli, anthropic_api_key: "configured" };
    expect(providerTransport(storedKey, "claude")).toBe("cli");

    const api = { ...storedKey, claude_access_mode: "api" as const };
    expect(providerTransport(api, "claude")).toBe("api");
  });

  it("forces the Google provider to the API transport", () => {
    // Google subscription dispatch (the agy CLI) is disabled; a stored
    // subscription mode from an older build is ignored.
    expect(providerTransport(settings(), "antigravity")).toBe("api");
    expect(
      providerTransport(settings({ google_api_key: "configured" }), "antigravity"),
    ).toBe("api");
  });

  it("round-trips select values and preserves inherit", () => {
    for (const value of ["automatic", "role:fast", "pinned:model-v2"]) {
      expect(encodeModelSelection(decodeModelSelection(value))).toBe(value);
    }
    expect(decodeModelSelection("inherit")).toBeUndefined();
    expect(encodeModelSelection(undefined)).toBe("inherit");
  });

  it("uses catalog-specific efforts with a fallback", () => {
    const catalog = {
      models: [{ id: "m", supported_efforts: ["low", "high"] }],
      roles: [],
    } as unknown as ModelCatalog;
    expect(effortOptions(catalog, { mode: "pinned", model: "m" }, ["medium"]))
      .toEqual(["low", "high"]);
    expect(effortOptions(undefined, { mode: "automatic" }, ["medium"]))
      .toEqual(["medium"]);
  });

  it("keeps legacy Merge defaults aligned with Sequential settings", () => {
    const legacy = settings({
      preferred_provider: "claude",
      default_sequential_agent: "codex",
      default_sequential_model_overrides: {
        "codex:cli": { mode: "pinned", model: "gpt-merge" },
      },
      default_sequential_effort_overrides: { "codex:cli": "high" },
    });
    expect(defaultMergeAgent(legacy)).toBe("codex");
    expect(defaultMergeModelOverrides(legacy)).toEqual(
      legacy.default_sequential_model_overrides,
    );
    expect(defaultMergeEffortOverrides(legacy)).toEqual(
      legacy.default_sequential_effort_overrides,
    );
  });
});
