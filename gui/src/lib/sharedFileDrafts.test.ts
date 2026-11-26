import { afterEach, expect, it, vi } from "vitest";
import {
  fileDraftKey,
  listFileDraftPaths,
  moveFileDrafts,
  readFileDraft,
  removeFileDraft,
  writeFileDraft,
} from "./sharedFileDrafts";

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

it("keeps corrupt drafts visible to warnings without blocking a valid draft move", () => {
  const scope = "corrupt-draft-test";
  localStorage.setItem(fileDraftKey(scope, "old/bad.tex"), "{");
  writeFileDraft(scope, {
    file: { path: "old/good.tex", hash: "h", text: "base" },
    draft: "unsaved",
  });
  expect(listFileDraftPaths(scope)).toEqual(["old/bad.tex", "old/good.tex"]);
  moveFileDrafts(scope, "old", "new");
  expect(readFileDraft(scope, "new/good.tex")?.draft).toBe("unsaved");
  expect(localStorage.getItem(fileDraftKey(scope, "old/bad.tex"))).toBe("{");
  expect(localStorage.getItem(fileDraftKey(scope, "old/good.tex"))).toBeNull();
});

it("lists and moves a draft retained only in memory after a storage failure", () => {
  const scope = "memory-draft-test";
  const value = {
    file: { path: "old/main.tex", hash: "original", text: "base" },
    draft: "unsaved",
  };
  const fail = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
    throw new Error("full");
  });
  expect(() => writeFileDraft(scope, value)).toThrow("full");
  expect(listFileDraftPaths(scope)).toEqual(["old/main.tex"]);
  expect(readFileDraft(scope, "old/main.tex")).toEqual(value);
  fail.mockRestore();
  moveFileDrafts(scope, "old", "new");
  expect(listFileDraftPaths(scope)).toEqual(["new/main.tex"]);
  expect(readFileDraft(scope, "new/main.tex")?.draft).toBe("unsaved");
  removeFileDraft(scope, "new/main.tex");
});

it("does not move a draft from a scope whose name extends this scope", () => {
  writeFileDraft("prefix.extra", {
    file: { path: "a.tex", hash: "h", text: "a" },
    draft: "b",
  });
  moveFileDrafts("prefix", "extra.a.tex", "wrong.tex");
  expect(readFileDraft("prefix.extra", "a.tex")?.draft).toBe("b");
  expect(listFileDraftPaths("prefix")).toEqual([]);
});
