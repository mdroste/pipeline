import { artifactClient } from "./artifactClient";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { invoke } from "@tauri-apps/api/core";
import { studioClient } from "./studioClient";
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
}
export interface FileAdapter {
  id: string;
  read: (path: string) => Promise<FilePreview>;
  save?: (file: FilePreview, text: string) => Promise<string>;
  reveal?: (path: string) => Promise<void>;
  external?: (path: string) => Promise<void>;
}
export interface WorkspaceFileScope {
  workspaceId: string;
  checkpointId?: string | null;
  revisionId?: string | null;
}
export function workspaceFileAdapter(scope: WorkspaceFileScope): FileAdapter {
  const request = (path: string) => ({
    ...scope,
    checkpointId: scope.checkpointId ?? null,
    revisionId: scope.revisionId ?? null,
    path,
  });
  return {
    id: `workspace:${scope.workspaceId}:${scope.revisionId ?? scope.checkpointId ?? "working"}`,
    read: (path) =>
      invoke<FilePreview>("workbench_file_read", { request: request(path) }),
    ...(!scope.revisionId
      ? {
          save: async (file: FilePreview, content: string) =>
            (
              await studioClient.mutate<{ hash: string }>(scope.workspaceId, {
                action: "saveText",
                checkpointId: scope.checkpointId ?? null,
                path: file.path,
                expectedHash: file.hash,
                content,
              })
            ).hash,
          external: async (path: string) => {
            const file = await invoke<FilePreview>("workbench_file_read", {
              request: request(path),
            });
            if (!file.externalPath)
              throw new Error("External file unavailable");
            await openExternal(file.externalPath);
          },
          reveal: (path: string) =>
            invoke<void>("workbench_file_reveal", { request: request(path) }),
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
      const base64 =
        content.kind === "pdf"
          ? await artifactClient.pdfBytes(runId, path)
          : content.base64;
      return {
        path,
        hash: "",
        bytes: content.bytes,
        text: content.text,
        base64,
        mime,
        editable: false,
        truncated: content.truncated,
        externalPath: content.abs_path,
      };
    },
    reveal: (path) => invoke("reveal_run_artifact", { runId, relPath: path }),
  };
}
