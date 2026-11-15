import { describe, expect, it } from "vitest";
import { legacyMathSpans } from "./legacyMath";
import { promoteLegacyMath } from "./legacyMathMarkdown";
import { normalizeMathDelimiters } from "./mathMarkdown";

const prepare = (source: string) =>
  promoteLegacyMath(normalizeMathDelimiters(source));
describe("legacy report notation", () => {
  it.each([
    ["The slope κ_w = cλ_w(1−β) holds.", "κ_{w} = cλ_{w}(1 − β)"],
    ["The shock sτ_t/(1−B_w) changes.", "sτ_{t} / (1 − B_{w})"],
    [
      "The estimate β_IV = βq/[s(1−β)+βq] changes.",
      "β_{IV} = βq / [s(1 − β) + βq]",
    ],
    ["The estimate σ̂_K = 0.67 changes.", String.raw`\hat{σ}_{K} = 0.67`],
    ["The terms Θ̂ and γ_f change.", String.raw`\hat{Θ}`],
    ["The variance σ_Y² changes.", "σ_{Y}^{2}"],
    ["The wage wᴸ = βW changes.", "w^{L} = βW"],
    ["The estimate Y_l changes.", "Y_{l}"],
    ["The elasticity α^L_f changes.", "α^{L}_{f}"],
    [
      "The equation (φπ−1)π + φy Ŷ = 0 holds.",
      String.raw`(φπ − 1)π + φy \hat{Y} = 0`,
    ],
  ])("recognizes whole expressions in %s", (source, latex) => {
    expect(legacyMathSpans(source).map((s) => s.latex)).toContain(latex);
    expect(prepare(source)).toContain(`$${latex}$`);
    expect(prepare(prepare(source))).toBe(prepare(source));
  });
  it.each([
    "A résumé in français; naïve café.",
    "file_name and x_variable and σ_file_name",
    "src/x_t.ts and /tmp/σ_t/data",
    "`κ_w = 2` and [σ_t](https://example.com/σ_t)",
    "```tex\nσ_t = 1\n```",
    "    σ_t = 1\n",
    "$\\kappa_w = 2$",
    "varsigma/(1+ν+α)",
    "σ_t/(1−B_w",
  ])("protects %s", (source) => expect(prepare(source)).toBe(source));
  it("preserves prefix and suffix currency and valid numeric math", () => {
    expect(prepare("per $1M (1996$), $5 to $8; $1+2$.")).toBe(
      "per \\$1M (1996\\$), \\$5 to \\$8; $1+2$.",
    );
    expect(prepare("MPK^$ by group costs $30.5M.")).toBe(
      "MPK^\\$ by group costs \\$30.5M.",
    );
  });
  it("recognizes notation beside escaped currency without rewriting escapes", () => {
    expect(prepare("The σ_t costs $5.")).toBe("The $σ_{t}$ costs \\$5.");
    expect(prepare(String.raw`Keep x\_t and \σ_t literal.`)).toBe(
      String.raw`Keep x\_t and \σ_t literal.`,
    );
  });
  it("keeps adjacent existing math spans separate", () => {
    expect(prepare(String.raw`σ_t$\beta$`)).toBe(
      String.raw`$σ_{t}$` + "\u2060" + String.raw`$\beta$`,
    );
  });
  it("keeps processing bounded on long or deeply nested input", () => {
    const source = "σ_t" + "(".repeat(2000) + "x".repeat(2000);
    expect(prepare(source)).toBe(source);
  });
});
