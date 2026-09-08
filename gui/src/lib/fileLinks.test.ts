import { describe, expect, it } from "vitest";
import { resolveFileLink } from "./fileLinks";
describe("scoped file links", () => {
  it("resolves sibling assets and source/page/heading locations", () => {
    expect(
      resolveFileLink("reports/review.md", "../code/estimate.do#L12-L19"),
    ).toEqual({ path: "code/estimate.do", line: 12 });
    expect(resolveFileLink("report.md", "paper.pdf#page=4")).toEqual({
      path: "paper.pdf",
      page: 4,
    });
    expect(resolveFileLink("notes/main.md", "appendix%20A.md#proof")).toEqual({
      path: "notes/appendix A.md",
      fragment: "proof",
    });
  });
  it.each([
    "../../secret",
    "%2fetc/passwd",
    "%2e%2e/secret",
    "file:///tmp/x",
    "javascript:alert(1)",
    "//host/x",
    "..\\x",
    "%00x",
    "%5cfoo",
    ".git/config",
    ".pipeline-tasks/a",
    "x?token=1",
    "#heading",
  ])("rejects external or unscoped target %s", (value) =>
    expect(resolveFileLink("note.md", value)).toBeNull(),
  );
});
