import { describe, expect, it } from "vitest";
import { connectionAvailability, providerAvailability } from "./connections";
import type { DepsReport, DepStatus, Settings } from "./types";

const settings = {
  claude_access_mode: "subscription",
  codex_access_mode: "subscription",
  anthropic_api_key: "",
  openai_api_key: "",
  google_api_key: "",
} as unknown as Settings;

const report = (...deps: Partial<DepStatus>[]) =>
  ({ ready: true, deps }) as unknown as DepsReport;

describe("connectionAvailability", () => {
  it("reads API connections from the saved keys, even before any report", () => {
    expect(connectionAvailability("google-api", settings, null)).toBe(
      "missing",
    );
    expect(
      connectionAvailability(
        "openai-api",
        { ...settings, openai_api_key: "key" },
        null,
      ),
    ).toBe("ready");
  });

  it("does not guess about a subscription before a report exists", () => {
    expect(connectionAvailability("claude", settings, null)).toBe("unknown");
    expect(connectionAvailability("codex", settings, null)).toBe("unknown");
  });

  it("uses Claude's own sign-in state in either access mode", () => {
    const signedIn = report({
      name: "Claude CLI",
      found: true,
      cli_auth_status: "signed_in",
    });
    const signedOut = report({
      name: "Claude CLI",
      found: true,
      cli_auth_status: "signed_out",
    });
    expect(connectionAvailability("claude", settings, signedIn)).toBe("ready");
    expect(connectionAvailability("claude", settings, signedOut)).toBe(
      "missing",
    );

    // With an API key saved, `found` describes the key rather than the CLI.
    const noCli = report({ name: "Claude CLI", found: true });
    expect(
      connectionAvailability(
        "claude",
        { ...settings, claude_access_mode: "api" },
        noCli,
      ),
    ).toBe("missing");
  });

  it("treats an unverified sign-in as unknown, not missing", () => {
    expect(
      connectionAvailability(
        "claude",
        settings,
        report({ name: "Claude CLI", found: true, cli_auth_status: "unknown" }),
      ),
    ).toBe("unknown");
  });

  it("checks the ChatGPT account only while the report describes it", () => {
    const signedOut = report({
      name: "ChatGPT (reviews)",
      found: true,
      authenticated: false,
    });
    expect(connectionAvailability("codex", settings, signedOut)).toBe(
      "missing",
    );
    expect(
      connectionAvailability(
        "codex",
        { ...settings, codex_access_mode: "api" },
        signedOut,
      ),
    ).toBe("unknown");
  });

  it("uses the local server's reachability", () => {
    expect(
      connectionAvailability(
        "local",
        settings,
        report({ name: "Local LLM server", found: false }),
      ),
    ).toBe("missing");
  });
});

describe("providerAvailability", () => {
  it("makes no claim for a caller with no readiness information", () => {
    expect(providerAvailability("antigravity", settings, undefined)).toBe(
      "unknown",
    );
  });

  it("follows the connection Reviews would use", () => {
    expect(
      providerAvailability(
        "claude",
        { ...settings, claude_access_mode: "api" },
        null,
      ),
    ).toBe("missing");
    expect(providerAvailability("antigravity", settings, null)).toBe("missing");
  });
});
