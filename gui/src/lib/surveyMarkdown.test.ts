import { describe, it, expect } from "vitest";
import { renderGenericSurvey } from "./surveyMarkdown";

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
