/** Compatibility owner for pre-shared Write drafts and per-document selection.
 * A differing legacy draft stays recoverable until its editor resolves it. */
import type { EditorFile } from "./studioTypes";

export interface LegacyManuscriptDraft {
  file: EditorFile;
  draft: string;
}
const baseKey = (workspace: string) => `pipeline.manuscriptDraft.${workspace}`;
const fileKey = (workspace: string, checkpoint: string, path: string) =>
  `${baseKey(workspace)}.${checkpoint || "working"}.${path}`;

function readDraft(key: string): LegacyManuscriptDraft | null {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "null");
    return value &&
      typeof value.draft === "string" &&
      typeof value.file?.path === "string" &&
      typeof value.file.hash === "string" &&
      typeof value.file.content === "string" &&
      (value.file.checkpointId === null ||
        typeof value.file.checkpointId === "string")
      ? value
      : null;
  } catch {
    return null;
  }
}

export function legacyManuscriptPaths(workspace: string, checkpoint = "") {
  const prefix = fileKey(workspace, checkpoint, "");
  return Array.from({ length: localStorage.length }, (_, i) =>
    localStorage.key(i),
  ).flatMap((key) =>
    key?.startsWith(prefix) ? [key.slice(prefix.length)] : [],
  );
}

export function legacyManuscriptDraft(
  workspace: string,
  checkpoint: string,
  path: string,
) {
  const key = fileKey(workspace, checkpoint, path);
  const raw = localStorage.getItem(key);
  const value: LegacyManuscriptDraft | null = JSON.parse(raw ?? "null");
  return {
    raw,
    value,
    remove() {
      localStorage.removeItem(key);
    },
    removeUnchanged() {
      if (localStorage.getItem(key) === raw) localStorage.removeItem(key);
      if (localStorage.getItem(baseKey(workspace)) === raw)
        localStorage.removeItem(baseKey(workspace));
    },
  };
}

export function manuscriptDraftStorage(
  workspace: string,
  documentId?: string | null,
) {
  const selection = documentId
    ? `${baseKey(workspace)}.document.${documentId}`
    : baseKey(workspace);
  const build = `pipeline.manuscriptBuild.${workspace}${documentId ? `.${documentId}` : ""}`;
  return {
    initial: () => readDraft(selection),
    readFile: (checkpoint: string, path: string) =>
      readDraft(fileKey(workspace, checkpoint, path)),
    location(): { path?: string; checkpointId?: string } {
      try {
        const value = JSON.parse(
          localStorage.getItem(`${selection}.location`) ?? "{}",
        );
        return {
          path: typeof value.path === "string" ? value.path : undefined,
          checkpointId:
            typeof value.checkpointId === "string"
              ? value.checkpointId
              : undefined,
        };
      } catch {
        return {};
      }
    },
    saveLocation(file: EditorFile) {
      localStorage.setItem(
        `${selection}.location`,
        JSON.stringify({ path: file.path, checkpointId: file.checkpointId }),
      );
    },
    forget(file?: EditorFile | null) {
      localStorage.removeItem(selection);
      if (file)
        localStorage.removeItem(
          fileKey(workspace, file.checkpointId ?? "", file.path),
        );
    },
    readBuild: () => localStorage.getItem(build),
    saveBuild: (id: string) => localStorage.setItem(build, id),
  };
}
