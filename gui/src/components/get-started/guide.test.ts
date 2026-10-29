import { describe, expect, it } from "vitest";
import type { DepStatus, DepsReport } from "../../lib/types";
import type { ToolchainReport } from "../../lib/workbench/toolchains";
import { makeSettings } from "../../test/settingsFixtures";
import {
  connectionRows,
  connectionSummary,
  groupTools,
  pdfRows,
  PERSONAS,
} from "./guide";

const dep = (overrides: Partial<DepStatus>): DepStatus => ({
  name: "Claude CLI",
  found: true,
  version: "1.0.0",
  path: "/usr/local/bin/claude",
  required: false,
  hint: "",
  ...overrides,
});

const report = (deps: DepStatus[]): DepsReport => ({ deps, ready: true });

const tool = (
  kind: ToolchainReport["tools"][number]["kind"],
  label: string,
) => ({
  kind,
  adapter: null,
  label,
  path: `/opt/${kind}`,
  source: "path" as const,
  version: "1.0",
  versionText: null,
  versionSource: "probe",
  sha256: null,
  sizeBytes: null,
  note: null,
});

describe("connection rows", () => {
  it("describes each connection's state and what it is for", () => {
    const settings = { ...makeSettings(), openai_api_key: "sk-test" };
    const rows = connectionRows({
      dependencies: report([
        dep({ cli_auth_status: "signed_out" }),
        dep({ name: "Local LLM server", found: true }),
      ]),
      dependenciesLoading: false,
      account: {
        account: {
          status: "chatgpt",
          email: null,
          planType: null,
          unsupportedAccountType: null,
          requiresOpenaiAuth: false,
        },
        epoch: 1,
        version: "0.147.0",
        loginInProgress: false,
      },
      accountError: null,
      settings,
    });
    expect(rows.map((row) => [row.name, row.status, row.tone])).toEqual([
      ["ChatGPT", "Signed in", "ok"],
      ["Claude Code", "Installed, not signed in", "attention"],
      ["API keys", "Saved: OpenAI", "ok"],
      ["Local model server", "Running", "ok"],
    ]);
    expect(rows[0].use).toMatch(/Conversations and Reviews/);
    expect(rows[2].use).toMatch(/Reviews only/);
    expect(connectionSummary(rows)).toMatch(/Conversations and Reviews now/);
  });

  it("reports missing connections without claiming readiness", () => {
    const rows = connectionRows({
      dependencies: report([
        dep({ found: false }),
        dep({ name: "Local LLM server", found: false }),
      ]),
      dependenciesLoading: false,
      account: null,
      accountError: "codex not found",
      settings: makeSettings(),
    });
    expect(rows.map((row) => row.status)).toEqual([
      "Could not check",
      "Not installed",
      "None saved",
      "Not running",
    ]);
    expect(connectionSummary(rows)).toMatch(
      /example Review needs no connection/,
    );
    const apiOnly = connectionRows({
      dependencies: null,
      dependenciesLoading: true,
      account: undefined,
      accountError: null,
      settings: { ...makeSettings(), google_api_key: "g" },
    });
    expect(apiOnly[0].status).toBe("Checking…");
    expect(apiOnly[1].status).toBe("Checking…");
    expect(connectionSummary(apiOnly)).toMatch(/run Reviews now/);
    // A finished request that returned nothing is not shown as loading.
    const unavailable = connectionRows({
      dependencies: null,
      dependenciesLoading: false,
      account: null,
      accountError: null,
      settings: null,
    });
    expect(unavailable.map((row) => row.status)).toEqual([
      "Could not check",
      "Not yet checked",
      "Could not check",
      "Not yet checked",
    ]);
  });
});

describe("tools and PDF readers", () => {
  it("puts the chosen kind of work first and names missing tools", () => {
    const found: ToolchainReport = {
      platform: "macos",
      tools: [tool("python", "Python 3.12"), tool("stata", "Stata 18 MP")],
      project: null,
      boundary: "Read-only detection.",
    };
    const stata = groupTools(found, "stata-latex");
    expect(stata.featured.map((item) => item.kind)).toEqual(["stata"]);
    expect(stata.others.map((item) => item.kind)).toEqual(["python"]);
    expect(stata.missing).toEqual([]);
    const notebooks = groupTools(found, "python-notebooks");
    expect(notebooks.featured.map((item) => item.kind)).toEqual(["python"]);
    expect(notebooks.missing).toEqual(["jupyter"]);
    const other = groupTools(found, null);
    expect(other.featured).toEqual([]);
    expect(other.others).toHaveLength(2);
    expect(PERSONAS["word-zotero"].templates).toEqual(["reference-check"]);
  });

  it("summarizes bundled PDF tools and the optional Full Parser", () => {
    expect(pdfRows(null)).toEqual([]);
    const rows = pdfRows(
      report([
        dep({ name: "pdftoppm" }),
        dep({ name: "pdftotext" }),
        dep({ name: "PaddleOCR-VL Full Parser", found: false }),
      ]),
    );
    expect(rows.map((row) => [row.name, row.status, row.tone])).toEqual([
      ["PDF text tools", "Ready", "ok"],
      ["Full Parser", "Not installed", "idle"],
    ]);
    expect(
      pdfRows(report([dep({ name: "pdftoppm", found: false })]))[0].status,
    ).toBe("Unavailable");
  });
});
