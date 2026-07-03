import { describe, it, expect } from "vitest";
import { isPaperOrientation } from "./types";

describe("isPaperOrientation", () => {
  it("accepts the paper orientation shape", () => {
    expect(
      isPaperOrientation({
        metadata: { title: "T", paper_type: "theory" },
        sections: [],
      }),
    ).toBe(true);
  });

  it("accepts partial paper shapes keyed by distinctive fields", () => {
    expect(isPaperOrientation({ formal_results: [] })).toBe(true);
    expect(isPaperOrientation({ stated_contribution: "..." })).toBe(true);
  });

  it("rejects custom survey schemas", () => {
    expect(
      isPaperOrientation({ overview: "a codebase", structure: [] }),
    ).toBe(false);
  });

  it("rejects null, undefined, and non-objects", () => {
    expect(isPaperOrientation(null)).toBe(false);
    expect(isPaperOrientation(undefined)).toBe(false);
    expect(isPaperOrientation("string")).toBe(false);
  });

  it("rejects metadata without paper_type", () => {
    expect(isPaperOrientation({ metadata: { title: "T" } })).toBe(false);
  });
});
