import { describe, it, expect } from "vitest";
import {
  parseIssues,
  extractJson,
  severityRank,
  detectReportIssues,
  renderSpecialistMarkdown,
  renderStructuredMarkdown,
} from "./issues";
import type { PipelineReport } from "./types";

describe("issues parsing", () => {
  it("extracts JSON from plain, fenced, and prose forms", () => {
    expect(extractJson('{"a":1}')).toEqual({ a: 1 });
    expect(extractJson("```json\n[1,2]\n```")).toEqual([1, 2]);
    expect(extractJson('the answer is {"ok": true} really')).toEqual({
      ok: true,
    });
    expect(extractJson("no json here")).toBeNull();
  });

  it("skips malformed candidates and handles mixed nesting", () => {
    expect(extractJson('bad [x] then {"items":[{"text":"} ]"}]}')).toEqual({
      items: [{ text: "} ]" }],
    });
    expect(extractJson("```json\n{bad}\n```\n```json\n[1,2]\n```")).toEqual([
      1, 2,
    ]);
  });

  it("parses an { issues: [...] } object", () => {
    const text = JSON.stringify({
      issues: [
        {
          id: "1",
          title: "Identification is weak",
          severity: "High",
          section: "4",
          body: "The IV...",
        },
        { id: "2", title: "Typo in Table 2", severity: "low", body: "Minor." },
      ],
    });
    const issues = parseIssues(text)!;
    expect(issues).toHaveLength(2);
    expect(issues[0].severity).toBe("high"); // normalized lowercase
    expect(issues[0].section).toBe("4");
    expect(issues[1].id).toBe("2");
  });

  it("parses a bare array and fills missing ids", () => {
    const issues = parseIssues('[{"title":"A"},{"title":"B"}]')!;
    expect(issues.map((i) => i.id)).toEqual(["1", "2"]);
  });

  it("parses the canonical findings envelope", () => {
    const issues = parseIssues(
      JSON.stringify({
        findings: [
          {
            id: "support-condition",
            title: "Support condition is missing",
            priority: "major",
            category: "Correctness",
            body: "Details",
          },
        ],
      }),
    )!;
    expect(issues[0]).toMatchObject({
      id: "support-condition",
      severity: "high",
      section: "Correctness",
    });
  });

  it("normalizes bounded evidence links and drops unsafe artifact paths", () => {
    const issues = parseIssues(
      JSON.stringify({
        issues: [
          {
            title: "Evidence-backed issue",
            evidence: [
              {
                page: 4,
                node_id: "node-12",
                description: "Proposition statement",
              },
              {
                artifact_path: "artifacts/figures/figure-2.png",
                asset_id: "figure-2",
              },
              { artifact_path: "../outside.txt", page: -1 },
            ],
          },
        ],
      }),
    )!;
    expect(issues[0].evidence).toEqual([
      { page: 4, nodeId: "node-12", description: "Proposition statement" },
      { assetId: "figure-2", artifactPath: "artifacts/figures/figure-2.png" },
    ]);
  });

  it("continues past valid non-issue JSON to a later issue candidate", () => {
    const issues = parseIssues(
      'metadata: {"model":"x"}\nresult: {"issues":[{"title":"Actual issue","severity":"major"}]}',
    )!;
    expect(issues).toHaveLength(1);
    expect(issues[0]).toMatchObject({
      title: "Actual issue",
      severity: "high",
    });
  });

  it("handles a large unmatched-brace prefix in linear time and recovers later JSON", () => {
    const text =
      "{".repeat(30_000) +
      JSON.stringify({ issues: [{ title: "Recovered issue" }] });
    expect(parseIssues(text)?.[0].title).toBe("Recovered issue");
  });

  it("bounds nesting state for a hostile near-limit unmatched prefix", () => {
    const text =
      "{".repeat(900_000) +
      JSON.stringify({
        issues: [{ title: "Recovered after hostile nesting" }],
      });
    expect(parseIssues(text)?.[0].title).toBe(
      "Recovered after hostile nesting",
    );
  });

  it("rejects issue arrays above the rendering safety cap", () => {
    const text = JSON.stringify({
      issues: Array.from({ length: 1_001 }, (_, i) => ({
        title: `Issue ${i}`,
      })),
    });
    expect(parseIssues(text)).toBeNull();
  });

  it("canonicalizes severity aliases and makes duplicate ids unique", () => {
    const issues = parseIssues(
      JSON.stringify({
        issues: [
          { id: "same", title: "A", severity: "critical" },
          { id: "same", title: "B", severity: "moderate" },
          { id: "same", title: "C", severity: "suggestion" },
        ],
      }),
    )!;
    expect(issues.map((issue) => issue.id)).toEqual([
      "same",
      "same#2",
      "same#3",
    ]);
    expect(issues.map((issue) => issue.severity)).toEqual([
      "high",
      "medium",
      "low",
    ]);
  });

  it("rejects non-issue JSON", () => {
    expect(parseIssues('{"metadata":{"title":"x"}}')).toBeNull();
    expect(parseIssues("prose, not json")).toBeNull();
    expect(parseIssues("[1,2,3]")).toBeNull(); // array of non-objects
  });

  it("orders severities via severityRank", () => {
    expect(severityRank("high")).toBeLessThan(severityRank("medium"));
    expect(severityRank("medium")).toBeLessThan(severityRank("low"));
    expect(severityRank("low")).toBeLessThan(severityRank("unknown"));
  });

  it("detectReportIssues picks the last issues-shaped step", () => {
    const report = {
      step_outputs: [
        {
          step_id: "a",
          step_label: "A",
          phase: "parallel",
          agent: "",
          raw_text: "prose",
        },
        {
          step_id: "s",
          step_label: "Synth",
          phase: "sequential",
          agent: "",
          raw_text: '{"issues":[{"title":"X"}]}',
        },
      ],
      report_date: "2026-01-01",
      paper_hash: "h",
      orientation: {},
    } as unknown as PipelineReport;
    const issues = detectReportIssues(report)!;
    expect(issues).toHaveLength(1);
    expect(issues[0].title).toBe("X");
  });

  it("uses the canonical published findings product", () => {
    const report = {
      products: {
        schema_version: 1,
        primary_step_id: "validate",
        findings: {
          schema_version: 1,
          source_step_id: "validate",
          source_step_label: "Validate",
          findings: [
            {
              id: "missing-support",
              title: "Support condition is missing",
              category: "Correctness",
              priority: "major",
              body: "The proposition needs an additional restriction.",
              sources: ["technical", "econometrics"],
              evidence: [{ source_path: "chapters/model.tex", line_start: 42 }],
            },
          ],
        },
      },
      step_outputs: [
        {
          step_id: "legacy",
          step_label: "Legacy",
          phase: "sequential",
          agent: "",
          raw_text: '{"issues":[{"title":"Wrong fallback"}]}',
        },
      ],
    } as unknown as PipelineReport;

    expect(detectReportIssues(report)).toEqual([
      {
        id: "missing-support",
        title: "Support condition is missing",
        severity: "high",
        section: "Correctness",
        body: "The proposition needs an additional restriction.",
        sources: ["technical", "econometrics"],
        evidence: [{ sourcePath: "chapters/model.tex", lineStart: 42 }],
      },
    ]);
  });

  it("does not infer a different product when a current report publishes no findings", () => {
    const report = {
      products: {
        schema_version: 1,
        primary_step_id: "validate",
        findings: {
          schema_version: 1,
          source_step_id: "validate",
          source_step_label: "Validate",
          findings: [],
        },
      },
      step_outputs: [
        {
          step_id: "legacy",
          step_label: "Legacy",
          phase: "sequential",
          agent: "",
          raw_text: '{"issues":[{"title":"Wrong fallback"}]}',
        },
      ],
    } as unknown as PipelineReport;

    expect(detectReportIssues(report)).toEqual([]);
  });
});

describe("renderSpecialistMarkdown", () => {
  it("renders specialist findings back to the referee format", () => {
    const text = JSON.stringify({
      findings: [
        {
          title: "Sign error",
          in_the_paper: "Claim",
          problem: "Analysis",
          consequence: "Effect",
          what_would_help: "Fix",
          evidence: [{ page: 12, quote: "the sign flips" }],
        },
      ],
    });
    const markdown = renderSpecialistMarkdown(text);
    expect(markdown).toContain("**#1. Sign error**");
    expect(markdown).toContain("- **The problem:** Analysis");
    expect(markdown).toContain("p. 12 · “the sign flips”");
  });

  it("renders an empty specialist report as the sentinel sentence", () => {
    expect(renderSpecialistMarkdown('{"findings": []}')).toBe(
      "No material issues identified.",
    );
  });

  it("rejects canonical findings and prose", () => {
    expect(
      renderSpecialistMarkdown(
        '{"findings":[{"id":"a","title":"T","body":"B"}]}',
      ),
    ).toBeNull();
    expect(renderSpecialistMarkdown("plain report text")).toBeNull();
  });
});

describe("renderStructuredMarkdown", () => {
  it("renders arrays of objects as tables and objects as sections", () => {
    const markdown = renderStructuredMarkdown(
      JSON.stringify({
        summary: "Two entries",
        rows: [
          { name: "a", value: 1 },
          { name: "b", value: 2 },
        ],
      }),
    );
    expect(markdown).toContain("**summary**: Two entries");
    expect(markdown).toContain("| name | value |");
    expect(markdown).toContain("| a | 1 |");
  });

  it("returns null for non-object payloads", () => {
    expect(renderStructuredMarkdown("[1,2]")).toBeNull();
    expect(renderStructuredMarkdown("prose")).toBeNull();
  });
});
