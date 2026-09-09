import { describe, expect, it } from "vitest";
import { absoluteLocalFilePath, resolveFileLink } from "./fileLinks";
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
  it("maps absolute project files into the registered root", () => {
    expect(
      resolveFileLink(
        "conversation.md",
        "/Users/example/Research%20Project/output/report.pdf#page=3",
        "/Users/example/Research Project",
      ),
    ).toEqual({ path: "output/report.pdf", page: 3 });
    expect(
      resolveFileLink(
        "conversation.md",
        "C:/Research/Paper/source/main.tex#L27",
        "c:\\research\\paper",
      ),
    ).toEqual({ path: "source/main.tex", line: 27 });
  });
  it.each([
    ["/Users/example/Other/report.pdf", "/Users/example/Research Project"],
    [
      "/Users/example/Research Project 2/report.pdf",
      "/Users/example/Research Project",
    ],
    [
      "/Users/example/Research Project/../../private.txt",
      "/Users/example/Research Project",
    ],
    ["D:/Research/Paper/report.pdf", "C:/Research/Paper"],
  ])(
    "rejects an absolute target outside its registered root: %s",
    (href, root) =>
      expect(resolveFileLink("conversation.md", href, root)).toBeNull(),
  );
  it("recognizes native absolute paths without accepting URLs or queries", () => {
    expect(absoluteLocalFilePath("/tmp/output%20file.pdf#page=2")).toBe(
      "/tmp/output file.pdf",
    );
    expect(absoluteLocalFilePath("C:/Temp/output.tex#L8")).toBe(
      "C:/Temp/output.tex",
    );
    expect(absoluteLocalFilePath("file:///tmp/output.pdf")).toBeNull();
    expect(absoluteLocalFilePath("//server/share/output.pdf")).toBeNull();
    expect(absoluteLocalFilePath("/tmp/output.pdf?token=1")).toBeNull();
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
