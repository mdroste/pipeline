import { describe, expect, it } from "vitest";
import { normalizeMathDelimiters } from "./mathMarkdown";

describe("normalizeMathDelimiters", () => {
  it("normalizes inline and display math", () => {
    expect(normalizeMathDelimiters("a \\(x\\)\n\\[\ny\n\\]\n")).toBe(
      "a $x$\n$$\ny\n$$\n",
    );
  });

  it("preserves inline and fenced code", () => {
    const input = "`\\(x\\)`\n\n```tex\n\\[y\\]\n```\n";
    expect(normalizeMathDelimiters(input)).toBe(input);
  });
});
