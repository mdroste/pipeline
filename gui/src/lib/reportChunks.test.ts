import { describe, expect, it } from "vitest";
import { splitReport } from "./reportChunks";

describe("report paging", () => {
  it("keeps every character when dividing a large report", () => {
    const text =
      "# Review\n\n" +
      Array.from(
        { length: 30 },
        (_, i) => `## Finding ${i}\n\nEvidence ${i}.\n\n`,
      ).join("");
    const chunks = splitReport(text, 90);
    expect(chunks.length).toBeGreaterThan(1);
    expect(chunks.map((chunk) => chunk.markdown).join("")).toBe(text);
    expect(chunks.at(-1)?.markdown).toContain("Evidence 29");
  });
  it("keeps fences intact and makes oversized blocks readable as source text", () => {
    const text =
      "# Report\n\n```js\n" +
      "const x = 1;\n".repeat(20) +
      "```\n\n## End\n\nLast sentence.";
    const chunks = splitReport(text, 100);
    expect(chunks.map((chunk) => chunk.markdown).join("")).toBe(text);
    expect(chunks.filter((chunk) => chunk.plain).length).toBeGreaterThan(1);
    expect(chunks.at(-1)).toMatchObject({ plain: false, title: "End" });
  });
  it("carries link definitions across parts", () => {
    const chunks = splitReport(
      "# First\n\nSee [the source][ref].\n\n" +
        "A paragraph.\n\n".repeat(10) +
        "[ref]: https://example.com\n",
      80,
    );
    expect(
      chunks.find((chunk) => chunk.markdown.includes("See [the source]"))
        ?.markdown,
    ).toContain("[ref]: https://example.com");
  });
});
