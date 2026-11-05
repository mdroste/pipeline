import { describe, expect, it } from "vitest";
import {
  invalidEntryName,
  listingFromPaths,
  movedPath,
  parentFolder,
  visibleRows,
} from "./fileTree";

describe("file tree", () => {
  it("builds folders from a fixed path list, folders first", () => {
    const paths = ["z.txt", "code/run.do", "code/data/a.csv", "b.txt"];
    expect(listingFromPaths(paths, "").entries.map((e) => e.name)).toEqual([
      "code",
      "b.txt",
      "z.txt",
    ]);
    expect(listingFromPaths(paths, "code").entries.map((e) => e.path)).toEqual([
      "code/data",
      "code/run.do",
    ]);
  });

  it("shows children only under expanded folders", () => {
    const paths = ["code/run.do", "code/data/a.csv", "b.txt"];
    const listings = {
      "": listingFromPaths(paths, ""),
      code: listingFromPaths(paths, "code"),
      "code/data": listingFromPaths(paths, "code/data"),
    };
    const shown = (open: string[]) =>
      visibleRows(listings, new Set(open)).map(
        (r) => `${r.depth}:${r.entry.path}`,
      );
    expect(shown([])).toEqual(["0:code", "0:b.txt"]);
    expect(shown(["code"])).toEqual([
      "0:code",
      "1:code/data",
      "1:code/run.do",
      "0:b.txt",
    ]);
    expect(shown(["code/data"])).toEqual(["0:code", "0:b.txt"]);
  });

  it("maps paths through a move without touching similar names", () => {
    expect(movedPath("a/b.txt", "a/b.txt", "c.txt")).toBe("c.txt");
    expect(movedPath("a/x/y.txt", "a", "z")).toBe("z/x/y.txt");
    expect(movedPath("ab/y.txt", "a", "z")).toBeNull();
    expect(parentFolder("a/b/c")).toBe("a/b");
    expect(parentFolder("c")).toBe("");
  });

  it("refuses names the project cannot hold", () => {
    expect(invalidEntryName("  ")).toBeTruthy();
    expect(invalidEntryName("..")).toBeTruthy();
    expect(invalidEntryName("a\\\\b")).toBeTruthy();
    expect(invalidEntryName("a:b")).toBeTruthy();
    expect(invalidEntryName("/a")).toBeTruthy();
    expect(invalidEntryName("notes/a.md")).toBeNull();
    expect(invalidEntryName("paper v2.tex")).toBeNull();
  });
});
