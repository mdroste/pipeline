import { describe, expect, it } from "vitest";
import {
  availableModes,
  defaultMode,
  fileKind,
  formatFileSize,
} from "./fileKinds";
import type { FilePreview } from "./fileWorkspaceClient";

const file = (overrides: Partial<FilePreview>): FilePreview => ({
  path: "a.txt",
  hash: "h",
  bytes: 1,
  text: "text",
  base64: null,
  mime: "text/plain",
  editable: true,
  truncated: false,
  externalPath: null,
  ...overrides,
});

describe("file kinds", () => {
  it("trusts the backend, then falls back to mime type and name", () => {
    expect(fileKind(file({ kind: "tiff" }))).toBe("tiff");
    expect(fileKind(file({ mime: "application/pdf", text: null }))).toBe("pdf");
    expect(fileKind(file({ mime: "image/png", text: null }))).toBe("image");
    expect(fileKind(file({ path: "a.csv" }))).toBe("table");
    expect(fileKind(file({ path: "a.dta", kind: "data", text: null }))).toBe(
      "data",
    );
    // A saved copy may arrive with its text; a data file still has no views.
    expect(
      availableModes(file({ path: "a.dta", kind: "data", text: "x,y\n" })),
    ).toEqual([]);
    expect(fileKind(file({ text: null }))).toBe("binary");
    expect(fileKind(file({}))).toBe("text");
  });

  it("offers the views each kind supports", () => {
    expect(availableModes(file({ text: null }))).toEqual([]);
    expect(
      availableModes(file({ path: "a.csv", kind: "table", text: null })),
    ).toEqual([]);
    expect(availableModes(file({ path: "a.md" }))).toEqual([
      "source",
      "preview",
      "split",
      "diff",
    ]);
    expect(availableModes(file({ path: "a.csv" }))).toEqual([
      "table",
      "source",
      "diff",
    ]);
    expect(availableModes(file({ path: "a.csv", editable: false }))).toEqual([
      "table",
      "source",
    ]);
    expect(availableModes(file({ kind: "html" }))).toEqual([
      "preview",
      "source",
      "diff",
    ]);
    expect(availableModes(file({ editable: false }))).toEqual(["source"]);
  });

  it("opens each kind in the view a reader expects", () => {
    expect(defaultMode(file({ path: "a.md" }))).toBe("preview");
    expect(defaultMode(file({ path: "a.csv" }))).toBe("table");
    expect(defaultMode(file({ kind: "notebook" }))).toBe("preview");
    expect(defaultMode(file({ mime: "image/svg+xml" }))).toBe("source");
    expect(defaultMode(file({}))).toBe("source");
  });

  it("formats sizes plainly", () => {
    expect(formatFileSize(12)).toBe("12 bytes");
    expect(formatFileSize(2048)).toBe("2.0 KB");
    expect(formatFileSize(5 * 1024 ** 3)).toBe("5.0 GB");
    expect(formatFileSize(300 * 1024 ** 2)).toBe("300 MB");
  });
});
