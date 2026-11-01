import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import ResearchTablePreview, {
  parseResearchTable,
} from "./ResearchTablePreview";
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
it("shows the source text when the table cannot be parsed", () => {
  render(<ResearchTablePreview text={'size\n5" sample'} path="data.csv" />);
  expect(screen.getByText(/Malformed quoted field/)).toBeVisible();
  expect(screen.getByText(/5" sample/)).toBeVisible();
});
