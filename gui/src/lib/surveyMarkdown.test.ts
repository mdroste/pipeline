import { describe, it, expect } from "vitest";
import { renderGenericSurvey, renderSurvey } from "./surveyMarkdown";

describe("renderGenericSurvey", () => {
  it("renders string values as titled sections", () => {
    const md = renderGenericSurvey({ overview: "A replication package for a paper." });
    expect(md).toContain("## Overview");
    expect(md).toContain("A replication package for a paper.");
  });

  it("renders arrays of objects as tables with the union of keys", () => {
    const md = renderGenericSurvey({
      components: [
        { path: "code/", role: "analysis scripts" },
        { path: "data/", role: "raw data", notes: "CSV" },
      ],
    });
    expect(md).toContain("## Components");
    expect(md).toContain("| Path | Role | Notes |");
    expect(md).toContain("| code/ | analysis scripts |  |");
    expect(md).toContain("| data/ | raw data | CSV |");
  });

  it("renders arrays of strings as bullet lists", () => {
    const md = renderGenericSurvey({ conventions: ["snake_case files", "figures in fig/"] });
    expect(md).toContain("- snake_case files");
    expect(md).toContain("- figures in fig/");
  });

  it("renders nested objects as key-value bullets", () => {
    const md = renderGenericSurvey({ metadata: { title: "T", pages: 12 } });
    expect(md).toContain("- **Title**: T");
    expect(md).toContain("- **Pages**: 12");
  });

  it("escapes pipes and newlines in table cells", () => {
    const md = renderGenericSurvey({
      key_files: [{ path: "a|b.txt", what_it_is: "multi\nline" }],
    });
    expect(md).toContain("a\\|b.txt");
    expect(md).toContain("multi line");
  });

  it("skips empty values and unknown-typed content instead of printing blanks", () => {
    const md = renderGenericSurvey({
      overview: "x",
      quality_notes: [],
      cross_references: null,
      blank: "   ",
    });
    expect(md).toContain("## Overview");
    expect(md).not.toContain("Quality Notes");
    expect(md).not.toContain("Cross References");
    expect(md).not.toContain("Blank");
  });

  it("falls back to a JSON fence for non-object surveys", () => {
    const md = renderGenericSurvey(["a", "b"]);
    expect(md).toContain("```json");
  });
});

describe("renderSurvey", () => {
  it("keeps the paper-specific orientation presentation", () => {
    const md = renderSurvey({
      metadata: {
        title: "A Paper",
        authors: ["Ada", "Grace"],
        date: null,
        paper_type: "theory",
        page_count: 12,
        has_appendix: true,
        has_online_appendix: false,
      },
      sections: [{ number: "1", title: "Model", page_start: 2, page_end: 5 }],
      formal_results: [],
      tables_figures: [],
      notation: [],
      stated_contribution: "A useful theorem.",
      key_references: [],
      extraction_quality_notes: [],
    });
    expect(md).toContain("# Orientation Map");
    expect(md).toContain("**Authors**: Ada, Grace");
    expect(md).toContain("| 1 | Model | 2–5 |");
    expect(md).toContain("## Stated Contribution");
  });

  it("renders an adaptive review plan and its selection reasons", () => {
    const md = renderSurvey({
      metadata: {
        title: "A Paper",
        authors: [],
        date: null,
        paper_type: "theory",
        page_count: 30,
        has_appendix: true,
        has_online_appendix: false,
      },
      review_plan: {
        primary_domain: "Economics",
        subject: "Quantitative macroeconomics",
        paper_forms: ["formal_theory", "quantitative_model"],
        methods: ["dynamic programming"],
        subject_specialist_ids: ["subject_economics_macro"],
        method_specialist_ids: ["formal_proofs", "quantitative_computation"],
        selection_notes: [
          { id: "formal_proofs", reason: "The main result depends on four propositions." },
        ],
        routing_uncertainty: [],
      },
      sections: [],
      formal_results: [],
      tables_figures: [],
      notation: [],
      stated_contribution: "A result.",
      key_references: [],
      extraction_quality_notes: [],
    });
    expect(md).toContain("## Detected Review Plan");
    expect(md).toContain("**Subject**: Quantitative macroeconomics");
    expect(md).toContain("**Formal Proofs**: The main result depends on four propositions.");
  });
});
