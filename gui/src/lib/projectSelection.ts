/** Pure file-change and byte-selection projections; no transport or UI state. */
import type {
  Checkpoint,
  DocumentSelection,
  DocumentView,
} from "./projectTypes";
export function changedPaths(checkpoint: Checkpoint): string[] {
  return [
    ...new Set([
      ...Object.keys(checkpoint.base),
      ...Object.keys(checkpoint.proposed),
    ]),
  ]
    .filter(
      (path) =>
        checkpoint.base[path]?.hash !== checkpoint.proposed[path]?.hash ||
        checkpoint.base[path]?.executable !==
          checkpoint.proposed[path]?.executable,
    )
    .sort();
}
/** DOM string offsets are UTF-16; stored locators are UTF-8 bytes. */
export function textSelection(
  text: string,
  start: number,
  end: number,
  view: DocumentView,
): DocumentSelection {
  const encoder = new TextEncoder();
  return {
    revisionId: view.revision.id,
    revisionHash: view.revision.contentHash,
    start: view.start + encoder.encode(text.slice(0, start)).length,
    end: view.start + encoder.encode(text.slice(0, end)).length,
    quote: text.slice(start, end),
    page: null,
    region: null,
  };
}
