import { describe, expect, it } from "vitest";
import {
  normalizeMathDelimiters,
  stripPresentationalHtml,
} from "./mathMarkdown";

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

  it("normalizes standalone LaTeX display environments", () => {
    const input = [
      "\\begin{equation}",
      "y = x + 1",
      "\\end{equation}",
      "",
      "\\begin{align*}",
      "a &= b \\\\",
      "c &= d",
      "\\end{align*}",
      "",
    ].join("\n");
    expect(normalizeMathDelimiters(input)).toBe(
      [
        "$$",
        "y = x + 1",
        "$$",
        "",
        "$$",
        "\\begin{aligned}",
        "a &= b \\\\",
        "c &= d",
        "\\end{aligned}",
        "$$",
        "",
      ].join("\n"),
    );
  });

  it("removes raw presentation tags without changing code or autolinks", () => {
    const input = [
      "R<sup>2</sup> and x<sub>t</sub><br>Next",
      "",
      "<div><span id=\"finding\">Finding</span></div>",
      "",
      "`<sup>literal</sup>` <https://example.com>",
      "",
      "```html",
      "<sup>literal block</sup>",
      "```",
      "",
    ].join("\n");

    expect(stripPresentationalHtml(input)).toBe(
      [
        "R2 and xt",
        "Next",
        "",
        "",
        "Finding",
        "",
        "",
        "`<sup>literal</sup>` <https://example.com>",
        "",
        "```html",
        "<sup>literal block</sup>",
        "```",
        "",
      ].join("\n"),
    );
  });
});
