import { artifactClient } from "./artifactClient";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { studioClient } from "./studioClient";
import type { FileKind } from "./fileKinds";
export interface FilePreview {
  path: string;
  hash: string;
  bytes: number;
  text: string | null;
  base64: string | null;
  mime: string;
  editable: boolean;
  truncated: boolean;
  externalPath: string | null;
  scopeRoot?: string | null;
  /** Viewer chosen by the backend from the file's bytes. */
  kind?: FileKind;
  /** Opens a `pipeline-file` URL. Viewers fall back to `base64` without one. */
  streamToken?: string | null;
  /** Compared with `FileAdapter.stat` to notice changes on disk. */
  stamp?: string | null;
  modifiedMs?: number | null;
  /** The file is not UTF-8, so `text` is a read-only approximation. */
  approximateText?: boolean;
  /** How many of the file's bytes `text` covers. */
  textBytes?: number | null;
}
export interface FileListEntry {
  name: string;
  path: string;
  kind: "file" | "folder" | "link" | "other";
  size: number;
  modifiedMs: number;
  ignored: boolean;
  executable: boolean;
}
export interface FileListing {
  directory: string;
  entries: FileListEntry[];
  truncated: boolean;
}
export interface FileStamp {
  path: string;
  kind: FileListEntry["kind"] | null;
  stamp: string | null;
  size: number;
  modifiedMs: number;
}
export interface FileTable {
  columns: string[];
  rows: (string | null)[][];
  start: number;
  pageSize: number;
  totalRows: number;
  sheet: string | null;
  sheets: string[];
  /** Set for a streamed CSV or TSV file, where `totalRows` is the rows read so
   * far. It is the file's total only when `endReached` is true. */
  preview?: TablePreview;
}
export interface TablePreview {
  /** Absent at the end of the file, and at a row too long to pass. */
  nextOffset: number | null;
  endReached: boolean;
  /** The widest row read, counting columns that are not shown. */
  columnCount: number;
  /** Cut cells as [data row, column]. A null row is the header. */
  clipped: [number | null, number][];
  /** Some bytes were not UTF-8, so those characters are replacements. */
  approximate: boolean;
  stamp: string;
}
/** Where a page of a CSV or TSV file continues, in the version that was read. */
export interface TableResume {
  offset: number;
  stamp: string;
}
export interface ImagePreview {
  base64: string;
  mime: string;
  width: number;
  height: number;
  sourceWidth: number;
  sourceHeight: number;
  pages: number;
}
export interface DocumentText {
  text: string;
  truncated: boolean;
}
export interface FileAdapter {
  id: string;
  read: (path: string) => Promise<FilePreview>;
  /** Bytes inline, for images embedded in rendered Markdown. */
  readInline?: (path: string) => Promise<FilePreview>;
  save?: (file: FilePreview, text: string) => Promise<string>;
  reveal?: (path: string) => Promise<void>;
  external?: (path: string) => Promise<void>;
  /** Stop serving a stream once its viewer closes. */
  release?: (token: string) => void;
  list?: (directory: string) => Promise<FileListing>;
  stat?: (paths: string[]) => Promise<FileStamp[]>;
  table?: (
    path: string,
    sheet: string | null,
    start: number,
    resume?: TableResume,
  ) => Promise<FileTable>;
  imagePreview?: (path: string) => Promise<ImagePreview>;
  documentText?: (path: string) => Promise<DocumentText>;
  createFile?: (path: string) => Promise<void>;
  createFolder?: (path: string) => Promise<void>;
  move?: (from: string, to: string) => Promise<void>;
  trash?: (path: string) => Promise<void>;
  /** Copy files from elsewhere on this computer into a project folder. */
  importFiles?: (directory: string, sources: string[]) => Promise<void>;
  /** Search the text of this folder's files. */
  search?: (query: string) => Promise<FileSearchResult>;
}
export interface FileSearchMatch {
  path: string;
  line: number;
  text: string;
}
export interface FileSearchResult {
  matches: FileSearchMatch[];
  searchedFiles: number;
  skippedFiles: number;
  truncated: boolean;
}
export const streamUrl = (token: string) =>
  convertFileSrc(token, "pipeline-file");

/**
 * A captured PDF for the reader. Its bytes arrive over the file scheme in one
 * read, so the token is released at once and no viewer has to hold it.
 */
export async function readCapturedPdf(
  adapter: FileAdapter,
): Promise<string | Uint8Array> {
  const file = await adapter.read("");
  if (file.streamToken) {
    try {
      return await fetchRange(streamUrl(file.streamToken), 0, file.bytes);
    } finally {
      adapter.release?.(file.streamToken);
    }
  }
  if (!file.base64) throw new Error("PDF unavailable");
  return file.base64;
}

/**
 * Bytes from a stream URL. The server may answer a long range in parts, so
 * this continues until the requested span is complete or the file ends.
 */
export async function fetchRange(
  url: string,
  start: number,
  end: number,
  signal?: AbortSignal,
): Promise<Uint8Array> {
  const result = new Uint8Array(end - start);
  let filled = 0;
  while (filled < result.length) {
    const response = await fetch(url, {
      headers: { Range: `bytes=${start + filled}-${end - 1}` },
      signal,
    });
    if (response.status !== 206 && response.status !== 200)
      throw new Error(
        response.status === 409 || response.status === 404
          ? "This file changed on disk. Reload it to continue."
          : `The file could not be read (${response.status}).`,
      );
    const part = new Uint8Array(await response.arrayBuffer());
    if (!part.length) break;
    result.set(
      part.subarray(0, Math.min(part.length, result.length - filled)),
      filled,
    );
    filled += part.length;
  }
  return filled >= result.length ? result : result.subarray(0, filled);
}
export interface WorkspaceFileScope {
  workspaceId: string;
  checkpointId?: string | null;
  revisionId?: string | null;
}
export function workspaceFileAdapter(scope: WorkspaceFileScope): FileAdapter {
  const checkpointId = scope.checkpointId ?? null;
  const { workspaceId } = scope;
  const request = (path: string) => ({
    ...scope,
    checkpointId,
    revisionId: scope.revisionId ?? null,
    path,
  });
  const mutate = (action: Parameters<typeof studioClient.mutate>[1]) =>
    studioClient.mutate<unknown>(workspaceId, action).then(() => undefined);
  return {
    id: `workspace:${workspaceId}:${scope.revisionId ?? checkpointId ?? "working"}`,
    read: (path) =>
      invoke<FilePreview>("workbench_file_open", { request: request(path) }),
    readInline: (path) =>
      invoke<FilePreview>("workbench_file_read", { request: request(path) }),
    release: (token) =>
      void invoke("workbench_file_release", { token }).catch(() => undefined),
    ...(!scope.revisionId
      ? {
          save: async (file: FilePreview, content: string) =>
            (
              await studioClient.mutate<{ hash: string }>(workspaceId, {
                action: "saveText",
                checkpointId,
                path: file.path,
                expectedHash: file.hash,
                content,
              })
            ).hash,
          external: (path: string) =>
            invoke<void>("workbench_file_external", { request: request(path) }),
          reveal: (path: string) =>
            invoke<void>("workbench_file_reveal", { request: request(path) }),
          list: (directory: string) =>
            invoke<FileListing>("workbench_file_list", {
              request: { workspaceId, directory, checkpointId },
            }),
          stat: (paths: string[]) =>
            invoke<FileStamp[]>("workbench_file_stat", {
              request: { workspaceId, paths, checkpointId },
            }),
          table: (
            path: string,
            sheet: string | null,
            start: number,
            resume?: TableResume,
          ) =>
            invoke<FileTable>("workbench_file_table", {
              request: {
                workspaceId,
                path,
                checkpointId,
                sheet,
                start,
                resume: resume ?? null,
              },
            }),
          imagePreview: (path: string) =>
            invoke<ImagePreview>("workbench_file_image_preview", {
              workspaceId,
              checkpointId,
              path,
            }),
          documentText: (path: string) =>
            invoke<DocumentText>("workbench_file_document_text", {
              workspaceId,
              checkpointId,
              path,
            }),
          search: (query: string) =>
            invoke<FileSearchResult>("workbench_file_search", {
              request: { workspaceId, checkpointId, query },
            }),
        }
      : {}),
    // File operations act on the working folder only.
    ...(!scope.revisionId && !checkpointId
      ? {
          createFile: (path: string) =>
            mutate({
              action: "createFile",
              checkpointId: null,
              path,
              base64: "",
            }),
          createFolder: (path: string) =>
            mutate({ action: "createFolder", path }),
          move: (from: string, to: string) =>
            mutate({ action: "moveEntry", from, to }),
          trash: (path: string) => mutate({ action: "trashEntry", path }),
          importFiles: async (directory: string, sources: string[]) => {
            for (const source of sources)
              await mutate({ action: "importFile", directory, source });
          },
        }
      : {}),
  };
}
export function artifactFileAdapter(
  runId: string,
  paths: string[],
): FileAdapter {
  const allowed = new Set(paths);
  return {
    id: `run:${runId}`,
    read: async (path) => {
      if (!allowed.has(path))
        throw new Error(
          "This file was not retained in the run's artifact manifest.",
        );
      const content = await artifactClient.read(runId, path);
      const suffix = path.split(".").pop()?.toLowerCase();
      const mime =
        content.kind === "pdf"
          ? "application/pdf"
          : content.kind === "image"
            ? ({
                jpg: "image/jpeg",
                jpeg: "image/jpeg",
                svg: "image/svg+xml",
                webp: "image/webp",
                gif: "image/gif",
              }[suffix ?? ""] ?? "image/png")
            : "text/plain";
      const viewable = content.kind === "pdf" || content.kind === "image";
      const streamToken = viewable
        ? await artifactClient.stream(runId, path).catch(() => null)
        : null;
      const base64 =
        content.kind === "pdf" && !streamToken
          ? await artifactClient.pdfBytes(runId, path)
          : content.base64;
      return {
        path,
        hash: "",
        bytes: content.bytes,
        text: content.text,
        base64,
        streamToken,
        mime,
        editable: false,
        truncated: content.truncated,
        externalPath: content.abs_path,
      };
    },
    release: (token) =>
      void invoke("workbench_file_release", { token }).catch(() => undefined),
    reveal: (path) => invoke("reveal_run_artifact", { runId, relPath: path }),
    external: (path) => invoke("open_run_artifact", { runId, relPath: path }),
  };
}
