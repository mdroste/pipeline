import { describe, expect, it } from "vitest";
import { bareMathRanges, normalizeUnicodeScripts } from "./bareMath";

describe("bare mathematical notation", () => {
  it.each([
    ["The coefficient β₁ ≈ −1.3 changes.", "β_{1} ≈ −1.3"],
    ["The FOCs imply (r+δ)pK/(wL)=a/b here.", "(r+δ)pK/(wL)=a/b"],
    ["Set m_j = χ^K ≈ 0.1 for firms.", "m_j = χ^K ≈ 0.1"],
    ["Use δa/[δa+(r+δ)b] instead.", "δa/[δa+(r+δ)b]"],
    ["Check x² + β₁₂ ≤ 3.", "x^{2} + β_{12} ≤ 3"],
  ])("recognizes %s", (text, formula) => {
    expect(bareMathRanges(text).map((range) => range.formula)).toEqual([
      formula,
    ]);
  });

  it.each([
    "Prices are $5 to $8.",
    "GDP and US exports",
    "SEs_for *x*",
    "the αlphabet",
    "2 + 2 = 4",
    "Read /tmp/x_i.csv",
    "Ambiguous α^K_life and σ̂_K stay as written.",
  ])("leaves prose and ambiguous notation alone: %s", (text) => {
    expect(bareMathRanges(text)).toEqual([]);
  });

  it("preserves Unicode script grouping", () => {
    expect(normalizeUnicodeScripts("xₜ₊₁ + yⁿ⁺¹")).toBe("x_{t+1} + y^{n+1}");
  });
});
