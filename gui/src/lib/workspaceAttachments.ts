// Importing research files into a project and referring to them from a
// conversation. Shared by the attach menu and by files dropped on the
// composer, so both follow one import path.

import { workbenchClient } from "./workbenchClient";
import { workbenchErrorMessage } from "./workbenchError";
import type { ContextItem, OpenResearchObject } from "./deskClient";
import type { PaperWithRevision } from "./workbenchTypes";

export const RESEARCH_FILE_EXTENSIONS = [
  "pdf",
  "docx",
  "tex",
  "md",
  "txt",
  "bib",
  "py",
  "r",
  "jl",
  "do",
  "json",
  "csv",
  "tsv",
];

export const fileName = (path: string) =>
  path.split(/[\\/]/).pop() || "Document";

/** Import each path as a project document; one failure does not stop the rest. */
export async function importResearchFiles(
  workspaceId: string,
  paths: string[],
  onImported?: (item: PaperWithRevision) => void,
): Promise<{ imported: PaperWithRevision[]; failures: string[] }> {
  const imported: PaperWithRevision[] = [];
  const failures: string[] = [];
  for (const path of paths) {
    const name = fileName(path);
    try {
      const item = await workbenchClient.importPaper({
        workspaceId,
        paperId: null,
        title: name,
        role: "other",
        path,
        operationId: `composer-import-${crypto.randomUUID()}`,
      });
      imported.push(item);
      onImported?.(item);
    } catch (cause) {
      failures.push(`${name}: ${workbenchErrorMessage(cause)}`);
    }
  }
  return { imported, failures };
}

export const isReadable = (item: PaperWithRevision) =>
  item.revision?.extraction.status === "complete";

/** The exact captured revision, as the conversation's source list refers to it. */
export const paperSource = (
  item: PaperWithRevision,
): OpenResearchObject | null =>
  item.revision && isReadable(item)
    ? {
        kind: "paper",
        id: item.revision.id,
        revision: item.revision.contentHash,
      }
    : null;

/**
 * Default roles for newly attached sources: data explains itself, the first
 * source in a conversation is the main paper, and the rest support it.
 */
export const asContextItems = (
  objects: OpenResearchObject[],
  existing = 0,
): ContextItem[] =>
  objects.map((object, index) => ({
    role:
      object.kind === "dataset"
        ? "data_dictionary"
        : existing + index
          ? "supporting"
          : "main",
    object,
  }));
