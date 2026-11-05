import { expect, it } from "vitest";
import { proseForDiff } from "./studioPresentation";

it("labels the prose transformation as an aid while retaining equations", () => {
  expect(
    proseForDiff(
      "\\section{Finding}\nThe result $a=b$ holds. \\cite{x}\n% hidden\nNext.",
    ),
  ).toContain("$a=b$");
  expect(proseForDiff("\\textbf{Text} \\label{x}% hidden")).toBe("Text");
});
