import type { ArtifactEntry, PageArtifactIndex } from "../../lib/artifactTypes";
export const MAX_INDEXED_PAGES = 5_000;
export const PAGE_SELECTION_PREFIX = "__page__:";

export function pageSelection(page: number): string {
  return `${PAGE_SELECTION_PREFIX}${page}`;
}

export function selectedPage(selection: string): number | null {
  if (!selection.startsWith(PAGE_SELECTION_PREFIX)) return null;
  const page = Number(selection.slice(PAGE_SELECTION_PREFIX.length));
  return Number.isInteger(page) && page > 0 ? page : null;
}

export function indexedPageForPath(
  relPath: string,
  index?: PageArtifactIndex | null,
): number | null {
  if (!index) return null;
  const match = relPath.match(/^artifacts\/pages\/page-(\d+)\.(?:jpe?g|png)$/i);
  if (!match) return null;
  const page = Number(match[1]);
  const count = Number.isSafeInteger(index.count)
    ? Math.min(Math.max(index.count, 0), MAX_INDEXED_PAGES)
    : 0;
  return Number.isInteger(page) && page > 0 && page <= count ? page : null;
}

export function pageNumber(entry: ArtifactEntry) {
  const match = `${entry.label} ${entry.rel_path}`.match(
    /(?:page[-_\s]*)(\d+)/i,
  );
  return match ? Number(match[1]) : Number.MAX_SAFE_INTEGER;
}
