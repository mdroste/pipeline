// Pure viewer selection. The backend sets `kind` from the file's bytes; adapters
// without one (run artifacts, test doubles) fall back to mime type and name.
import type { FilePreview } from "./fileWorkspaceClient";

export type FileKind =
  | "text"
  | "pdf"
  | "image"
  | "tiff"
  | "audio"
  | "video"
  | "html"
  | "notebook"
  | "table"
  | "docx"
  | "data"
  | "binary";
export type FileMode = "source" | "preview" | "split" | "diff" | "table";

const KINDS: readonly string[] = [
  "text",
  "pdf",
  "image",
  "tiff",
  "audio",
  "video",
  "html",
  "notebook",
  "table",
  "docx",
  "data",
  "binary",
];
export function fileKind(file: FilePreview): FileKind {
  if (file.kind && KINDS.includes(file.kind)) return file.kind as FileKind;
  if (file.mime === "application/pdf") return "pdf";
  if (file.mime.startsWith("image/")) return "image";
  if (file.text === null) return "binary";
  return /\.(csv|tsv)$/i.test(file.path) ? "table" : "text";
}
const isMarkdown = (path: string) => /\.(md|markdown|rmd|qmd)$/i.test(path);
const isSvg = (file: FilePreview) =>
  file.mime === "image/svg+xml" && file.text !== null;

/** The toolbar's views of a file with text. Files without text have none. */
export function availableModes(file: FilePreview): FileMode[] {
  if (file.text === null || file.kind === "data") return [];
  if (isMarkdown(file.path) || isSvg(file))
    return ["source", "preview", "split", "diff"];
  const changes: FileMode[] = file.editable ? ["diff"] : [];
  const kind = fileKind(file);
  if (kind === "table") return ["table", "source", ...changes];
  if (kind === "html" || kind === "notebook")
    return ["preview", "source", ...changes];
  return ["source", ...changes];
}
export function defaultMode(file: FilePreview): FileMode {
  if (isMarkdown(file.path)) return "preview";
  return isSvg(file) ? "source" : (availableModes(file)[0] ?? "source");
}
export function modeLabel(mode: FileMode): string {
  return mode === "diff" ? "Changes" : mode[0].toUpperCase() + mode.slice(1);
}
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} bytes`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = -1;
  do {
    value /= 1024;
    unit++;
  } while (value >= 1024 && unit < units.length - 1);
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`;
}
