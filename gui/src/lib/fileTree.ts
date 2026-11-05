// Pure helpers for the file browser's tree.
import type { FileListEntry, FileListing } from "./fileWorkspaceClient";

const compare = (a: FileListEntry, b: FileListEntry) =>
  Number(a.kind !== "folder") - Number(b.kind !== "folder") ||
  a.name.toLowerCase().localeCompare(b.name.toLowerCase());

/** One directory of a fixed path list, for adapters that cannot list folders. */
export function listingFromPaths(
  paths: string[],
  directory: string,
): FileListing {
  const prefix = directory ? `${directory}/` : "";
  const entries = new Map<string, FileListEntry>();
  for (const path of paths) {
    if (!path.startsWith(prefix)) continue;
    const [name, ...rest] = path.slice(prefix.length).split("/");
    if (!name || entries.has(name)) continue;
    entries.set(name, {
      name,
      path: `${prefix}${name}`,
      kind: rest.length ? "folder" : "file",
      size: 0,
      modifiedMs: 0,
      ignored: false,
      executable: false,
    });
  }
  return {
    directory,
    entries: [...entries.values()].sort(compare),
    truncated: false,
  };
}
export interface TreeRow {
  entry: FileListEntry;
  depth: number;
}
/** The rows the tree shows: each expanded folder's children, in order. */
export function visibleRows(
  listings: Record<string, FileListing>,
  expanded: ReadonlySet<string>,
  directory = "",
  depth = 0,
): TreeRow[] {
  const rows: TreeRow[] = [];
  for (const entry of listings[directory]?.entries ?? []) {
    rows.push({ entry, depth });
    if (entry.kind === "folder" && expanded.has(entry.path))
      rows.push(...visibleRows(listings, expanded, entry.path, depth + 1));
  }
  return rows;
}
export const parentFolder = (path: string) =>
  path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
export const baseName = (path: string) => path.split("/").pop() ?? path;
export const joinPath = (directory: string, name: string) =>
  directory ? `${directory}/${name}` : name;

/** `path` after `from` became `to`, or null when the move does not touch it. */
export function movedPath(
  path: string,
  from: string,
  to: string,
): string | null {
  if (path === from) return to;
  return path.startsWith(`${from}/`) ? `${to}${path.slice(from.length)}` : null;
}
/** A name for a new item, or why it cannot be one. */
export function invalidEntryName(name: string): string | null {
  const value = name.trim();
  if (!value) return "Enter a name.";
  if (value === "." || value === "..") return "Choose another name.";
  if (/[\\:]/.test(value) || /^\/|\/$/.test(value))
    return "Names cannot contain a backslash or colon.";
  return null;
}
