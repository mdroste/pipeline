import { describe, expect, it } from "vitest";
import {
  canRenderLatex,
  latexToReadableText,
  repairLatexExpression,
  remarkRepairMath,
} from "./mathRepair";

describe("math repair", () => {
  it("applies only deterministic syntax-level repairs", () => {
    expect(
      repairLatexExpression(
        String.raw`\begin{split}\bm{x} &= 1 \label{eq:x}\\ y &= \eqref{eq:y}\end{split}`,
      ),
    ).toBe(
      String.raw`\begin{aligned}\bm{x} &= 1 \\ y &= \text{(eq:y)}\end{aligned}`,
    );
  });

  it("validates configured compatibility macros with KaTeX", () => {
    expect(canRenderLatex(String.raw`\bm{x}\in\mathbbm{R}`)).toBe(true);
    expect(canRenderLatex(String.raw`\notARealCommand{x}`)).toBe(false);
  });

  it("creates a readable fallback without inventing mathematical content", () => {
    expect(
      latexToReadableText(
        String.raw`\frac{\alpha_t}{\sqrt{x}} \notARealCommand{z}`,
      ),
    ).toBe("(α_t)/(√(x)) notARealCommandz");
  });

  it("promotes high-confidence standalone and inline bare TeX", () => {
    const tree = {
      type: "root",
      children: [
        {
          type: "paragraph",
          children: [
            {
              type: "text",
              value: String.raw`x_t = \rho x_{t-1} + \varepsilon_t`,
            },
          ],
        },
        {
          type: "paragraph",
          children: [
            {
              type: "text",
              value: String.raw`The coefficient \alpha_t changes.`,
            },
          ],
        },
      ],
    };

    remarkRepairMath()(tree);

    expect(tree.children[0]).toMatchObject({
      type: "math",
      value: String.raw`x_t = \rho x_{t-1} + \varepsilon_t`,
    });
    expect(
      tree.children[1].children?.map(({ type, value }) => ({ type, value })),
    ).toEqual([
      { type: "text", value: "The coefficient " },
      { type: "inlineMath", value: String.raw`\alpha_t` },
      { type: "text", value: " changes." },
    ]);
  });
});
