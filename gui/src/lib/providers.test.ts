import { describe, expect, it } from "vitest";
import type { ModelCatalog, Settings } from "./types";
import {
  decodeModelSelection,
  effortOptions,
  encodeModelSelection,
  providerSelection,
  providerTransport,
  withProviderSelection,
} from "./providers";

function settings(overrides: Partial<Settings> = {}): Settings {
  return {
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
  it("keeps CLI and API selections independent", () => {
    const cli = settings({
      claude_cli_model_selection: { mode: "role", role: "balanced" },
      claude_api_model_selection: { mode: "pinned", model: "api-model" },
    });
    expect(providerTransport(cli, "claude")).toBe("cli");
    expect(providerSelection(cli, "claude")).toEqual({ mode: "role", role: "balanced" });

    const api = { ...cli, anthropic_api_key: "configured" };
    expect(providerTransport(api, "claude")).toBe("api");
    expect(providerSelection(api, "claude")).toEqual({ mode: "pinned", model: "api-model" });
    expect(withProviderSelection(api, "claude", { mode: "automatic" }))
      .toMatchObject({ claude_api_model_selection: { mode: "automatic" }, claude_model: "" });
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
});
