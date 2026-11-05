import { describe, expect, it } from "vitest";
import {
  ADAPTER_HELP,
  ADAPTER_OPTIONS,
  adapterTemplate,
  guessPlatform,
  isUntouchedTemplate,
  stataUsesLauncher,
  toolsForAdapter,
  withDetectedProgram,
  type DetectedToolchain,
  type ToolchainReport,
} from "./toolchains";

function tool(
  kind: DetectedToolchain["kind"],
  path: string,
  label = kind,
): DetectedToolchain {
  return {
    kind,
    adapter: null,
    label,
    path,
    source: "standardLocation",
    version: null,
    versionText: null,
    versionSource: "not checked",
    sha256: null,
    sizeBytes: null,
    note: null,
  };
}

const report: ToolchainReport = {
  platform: "macos",
  tools: [
    tool("stata", "/Applications/StataNow/StataMP.app/Contents/MacOS/stata-mp"),
    tool("python", "/research/.venv/bin/python"),
    tool("jupyter", "/research/.venv/bin/jupyter"),
    tool("matlab", "/Applications/MATLAB_R2024b.app/bin/matlab"),
  ],
  project: null,
  boundary: "Read only.",
};

describe("execution adapter templates", () => {
  it("offers every adapter with plain-language help", () => {
    expect(ADAPTER_OPTIONS.map((option) => option.value)).toEqual([
      "command",
      "latex",
      "stata",
      "r",
      "python",
      "julia",
      "notebook",
      "quarto",
    ]);
    for (const option of ADAPTER_OPTIONS)
      expect(ADAPTER_HELP[option.value].length).toBeGreaterThan(40);
    expect(ADAPTER_HELP.stata).not.toContain("oldstata");
  });

  it("uses the platform's Stata batch flag and declares the log", () => {
    expect(adapterTemplate("stata", "macos")).toEqual({
      argv: ["stata-mp", "-b", "do", "main.do"],
      outputs: ["main.log"],
    });
    expect(adapterTemplate("stata", "windows").argv).toEqual([
      "StataMP-64.exe",
      "/e",
      "do",
      "main.do",
    ]);
    expect(adapterTemplate("notebook", "linux").outputs).toEqual([
      "analysis.executed.ipynb",
    ]);
  });

  it("recognizes unedited templates on any platform", () => {
    expect(isUntouchedTemplate('["/usr/bin/true"]', "command", "argv")).toBe(
      true,
    );
    expect(
      isUntouchedTemplate(
        JSON.stringify(["StataMP-64.exe", "/e", "do", "main.do"]),
        "stata",
        "argv",
      ),
    ).toBe(true);
    expect(isUntouchedTemplate('["stata-mp", "-b"]', "stata", "argv")).toBe(
      false,
    );
    expect(isUntouchedTemplate("main.log", "stata", "outputs")).toBe(true);
    expect(isUntouchedTemplate("tables/main.csv", "stata", "outputs")).toBe(
      false,
    );
  });
});

describe("detected tools", () => {
  it("matches tools to the adapter that runs them", () => {
    expect(toolsForAdapter(report, "stata").map((t) => t.kind)).toEqual([
      "stata",
    ]);
    expect(toolsForAdapter(report, "notebook").map((t) => t.kind)).toEqual([
      "jupyter",
    ]);
    expect(toolsForAdapter(report, "latex")).toEqual([]);
    expect(toolsForAdapter(report, "command")).toHaveLength(4);
  });

  it("fills a template but keeps a researcher's own script and options", () => {
    const stata = report.tools[0];
    expect(
      JSON.parse(
        withDetectedProgram(
          JSON.stringify(["stata-mp", "-b", "do", "main.do"]),
          "stata",
          stata,
          "macos",
        ),
      ),
    ).toEqual([stata.path, "-b", "do", "main.do"]);
    expect(
      JSON.parse(
        withDetectedProgram(
          '["stata-se", "-q", "-b", "do", "code/clean.do"]',
          "stata",
          stata,
          "macos",
        ),
      ),
    ).toEqual([stata.path, "-q", "-b", "do", "code/clean.do"]);
    // A custom launcher becomes a direct batch invocation.
    expect(
      JSON.parse(
        withDetectedProgram(
          JSON.stringify(["/bin/zsh", "-lic", "oldstata -q -b do main.do"]),
          "stata",
          stata,
          "macos",
        ),
      ),
    ).toEqual([stata.path, "-b", "do", "main.do"]);
    expect(
      JSON.parse(
        withDetectedProgram("not json", "python", report.tools[1], "linux"),
      ),
    ).toEqual(["/research/.venv/bin/python", "analysis.py"]);
    expect(
      JSON.parse(
        withDetectedProgram(
          '["/usr/bin/true"]',
          "command",
          report.tools[1],
          "linux",
        ),
      ),
    ).toEqual(["/research/.venv/bin/python"]);
  });

  it("identifies Stata custom launchers from the recorded command", () => {
    expect(
      stataUsesLauncher(["/bin/zsh", "-lic", "oldstata -q -b do a.do"]),
    ).toBe(true);
    expect(stataUsesLauncher(["/bin/bash", "-lic", "stata -b do a.do"])).toBe(
      true,
    );
    expect(stataUsesLauncher(["stata-mp", "-b", "do", "a.do"])).toBe(false);
    expect(stataUsesLauncher([])).toBe(false);
  });

  it("guesses the platform for templates before detection", () => {
    expect(guessPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe(
      "windows",
    );
    expect(guessPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)")).toBe(
      "macos",
    );
    expect(guessPlatform("Mozilla/5.0 (X11; Linux x86_64)")).toBe("linux");
  });
});
