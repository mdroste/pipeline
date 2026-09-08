import { expect, it } from "vitest";
import { applyFileHunks, fileHunks } from "./fileDiff";
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
