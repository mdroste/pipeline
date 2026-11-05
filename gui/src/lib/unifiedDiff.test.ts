import { describe, expect, it } from "vitest";
import { conflictRows, newFileRows, parseUnifiedDiff } from "./unifiedDiff";

describe("parseUnifiedDiff", () => {
  it("numbers both sides and drops Git's file header", () => {
    const rows = parseUnifiedDiff(
      [
        "diff --git a/paper.tex b/paper.tex",
        "index 111..222 100644",
        "--- a/paper.tex",
        "+++ b/paper.tex",
        "@@ -2,3 +2,3 @@ \\section{Data}",
        " kept",
        "-old",
        "+new",
        "\\ No newline at end of file",
        "",
      ].join("\n"),
    );
    expect(rows.map((row) => [row.kind, row.oldLine, row.newLine])).toEqual([
      ["hunk", null, null],
      ["context", 2, 2],
      ["del", 3, null],
      ["add", null, 3],
      ["note", null, null],
    ]);
    expect(rows[0].text).toBe("\\section{Data}");
    expect(rows[2].text).toBe("old");
  });

  it("keeps lines that look like headers once inside a hunk", () => {
    const rows = parseUnifiedDiff(
      "@@ -1 +1 @@\n--- a dash line\n+++ a plus line\n",
    );
    expect(rows.map((row) => row.text)).toEqual([
      "",
      "-- a dash line",
      "++ a plus line",
    ]);
  });

  it("reports a permissions change that has no lines to show", () => {
    const patch = (from: string, to: string) =>
      `diff --git a/run.sh b/run.sh\nold mode ${from}\nnew mode ${to}\n`;
    const rows = parseUnifiedDiff(patch("100644", "100755"));
    expect(rows.map((row) => row.kind)).toEqual(["note"]);
    expect(rows[0].text).toContain("can now be run");
    expect(parseUnifiedDiff(patch("100755", "100644"))[0].text).toContain(
      "can no longer be run",
    );
  });

  it("keeps a permissions change above the changed lines", () => {
    const rows = parseUnifiedDiff(
      "diff --git a/run.sh b/run.sh\nold mode 100644\nnew mode 100755\nindex 1..2\n--- a/run.sh\n+++ b/run.sh\n@@ -1 +1 @@\n-a\n+b\n",
    );
    expect(rows.map((row) => row.kind)).toEqual(["note", "hunk", "del", "add"]);
  });

  it("reports a rename that left the contents alone", () => {
    const header =
      "diff --git a/keep.txt b/moved.txt\nsimilarity index 100%\nrename from keep.txt\nrename to moved.txt\n";
    expect(parseUnifiedDiff(header).map((row) => row.text)).toEqual([
      "Renamed. The contents did not change.",
    ]);
    const edited = parseUnifiedDiff(`${header}@@ -1 +1 @@\n-a\n+b\n`);
    expect(edited.some((row) => row.kind === "note")).toBe(false);
  });

  it("returns nothing for empty text", () => {
    expect(parseUnifiedDiff("")).toEqual([]);
  });
});

it("shows a new file as numbered additions", () => {
  expect(
    newFileRows("one\ntwo\n").map((row) => [row.kind, row.newLine]),
  ).toEqual([
    ["add", 1],
    ["add", 2],
  ]);
});

it("labels the two sides of a conflict", () => {
  const rows = conflictRows(
    "before\n<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> origin/main\nafter",
  );
  expect(rows.map((row) => [row.kind, row.text])).toEqual([
    ["context", "before"],
    ["hunk", "This branch"],
    ["del", "mine"],
    ["hunk", "Incoming"],
    ["add", "theirs"],
    ["hunk", "End of conflict"],
    ["context", "after"],
  ]);
});
