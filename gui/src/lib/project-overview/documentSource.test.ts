import { expect, it } from "vitest";
import { documentSourcePath } from "./documentSource";
import type { PaperWithRevision } from "../workbench/researchTypes";
const document = (entrypoint: string, inputKind = "file") =>
  ({ revision: { entrypoint, inputKind } }) as PaperWithRevision;
it("matches canonical Windows and UNC roots without losing their namespace", () => {
  expect(
    documentSourcePath(
      document("C:\\Research\\Paper\\Main.tex"),
      "\\\\?\\C:\\Research\\Paper",
      ["main.tex"],
    ),
  ).toBe("main.tex");
  expect(
    documentSourcePath(
      document("\\\\server\\research\\Paper", "source_tree"),
      "\\\\?\\UNC\\server\\research\\Paper",
      ["main.tex", "section.tex"],
    ),
  ).toBe("main.tex");
});
it("keeps independent roots and ambiguous documents separate", () => {
  expect(
    documentSourcePath(document("/paper/slides", "source_tree"), "/paper", [
      "main.tex",
      "slides/talk.tex",
    ]),
  ).toBe("slides/talk.tex");
  expect(
    documentSourcePath(document("/paper", "source_tree"), "/paper", [
      "paper.tex",
      "slides.tex",
    ]),
  ).toBeNull();
  expect(
    documentSourcePath(document("/other/main.tex"), "/paper", []),
  ).toBeNull();
  expect(documentSourcePath(document("/paper/draft#1.tex"), "/paper", [])).toBe(
    "draft#1.tex",
  );
});
