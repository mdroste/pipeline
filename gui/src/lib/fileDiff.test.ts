import { expect, it } from "vitest";
import { applyFileHunks, fileHunks, type FileHunk } from "./fileDiff";
it("selects independent edits without losing CRLF, Unicode or final newlines", () => {
  const before = "α\r\nold one\r\nstable\r\nold two";
  const after = "α\r\nnew one\r\nstable\r\nnew two\r\n";
  const hunks = fileHunks(before, after);
  expect(hunks).toHaveLength(2);
  expect(applyFileHunks(before, hunks, [0])).toBe(
    "α\r\nnew one\r\nstable\r\nold two",
  );
  expect(
    applyFileHunks(
      before,
      hunks,
      hunks.map((h) => h.id),
    ),
  ).toBe(after);
  expect(applyFileHunks(before, hunks, [])).toBe(before);
  expect(() => applyFileHunks("changed", hunks, [0])).toThrow();
});
it("assembles many edits once, including adjacent insertions in their original order", () => {
  const before = "0123456789".repeat(10000);
  const hunks: FileHunk[] = Array.from({ length: 1000 }, (_, id) => ({
    id,
    from: id * 100,
    to: id * 100 + 1,
    before: "0",
    after: "α😀",
    line: id + 1,
  }));
  const selected = hunks.filter((h) => h.id % 2 === 0).map((h) => h.id);
  expect(applyFileHunks(before, hunks, selected)).toBe(
    Array.from(
      { length: 1000 },
      (_, i) =>
        `${i % 2 === 0 ? "α😀" : "0"}${before.slice(i * 100 + 1, (i + 1) * 100)}`,
    ).join(""),
  );
  expect(
    applyFileHunks(
      "x",
      [
        { id: 0, from: 0, to: 0, before: "", after: "a", line: 1 },
        { id: 1, from: 0, to: 0, before: "", after: "b", line: 1 },
      ],
      [0, 1],
    ),
  ).toBe("abx");
});
it("rejects stale, overlapping and invalid hunks even when none are selected", () => {
  const hunk: FileHunk = {
    id: 0,
    from: 0,
    to: 1,
    before: "x",
    after: "y",
    line: 1,
  };
  for (const hunks of [
    [{ ...hunk, before: "stale" }],
    [hunk, hunk],
    [{ ...hunk, from: -1 }],
    [{ ...hunk, to: 0.5 }],
  ]) {
    expect(() => applyFileHunks("x", hunks, [])).toThrow("source changed");
  }
});
it.each([
  ["", "insert\n"],
  ["remove\n", ""],
  ["last", "last\n"],
  ["a\nb\na\n", "a\na\n"],
])("reconstructs boundary edit %j", (before, after) => {
  const h = fileHunks(before, after);
  expect(
    applyFileHunks(
      before,
      h,
      h.map((h) => h.id),
    ),
  ).toBe(after);
});
