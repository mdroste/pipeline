import { expect, it } from "vitest";
import { parseResearchTable } from "./ResearchTablePreview";
it("preserves quoted delimiters, multiline cells, escaped quotes, and numeric text", () => {
  expect(
    parseResearchTable(
      'name,value\r\n"a,b","1.00"\r\n"two\nlines","say ""yes"""',
      ",",
    ),
  ).toEqual({
    rows: [
      ["name", "value"],
      ["a,b", "1.00"],
      ["two\nlines", 'say "yes"'],
    ],
    truncated: false,
    error: null,
  });
});
it("bounds rows and exposes malformed input", () => {
  expect(parseResearchTable("a\nb\nc", ",", 2)).toMatchObject({
    rows: [["a"], ["b"]],
    truncated: true,
  });
  expect(parseResearchTable('"unfinished', ",").error).toBeTruthy();
});
