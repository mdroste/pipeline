// Files the researcher unchecked, by project, for as long as the app runs. This
// outlives the page on purpose: Changes unmounts for History, and the whole page
// unmounts when the project changes tabs, as Open in Files does. Losing the list
// would put a file the researcher left out back into the next commit.
//
// Paths stay here after they leave the list of changes. A file that disappears
// for a moment, as it can across a branch switch, must come back still unchecked.
import { useCallback, useSyncExternalStore } from "react";
import type { ChangedFile } from "../../lib/versionControlClient";

const NONE: ReadonlySet<string> = new Set();
const unchecked = new Map<string, ReadonlySet<string>>();
const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The folders above a path. `.` is the whole project folder. */
function foldersAbove(path: string) {
  const folders = ["."];
  for (
    let end = path.indexOf("/");
    end !== -1;
    end = path.indexOf("/", end + 1)
  )
    folders.push(path.slice(0, end));
  return folders;
}

const inside = (path: string, folder: string) =>
  path !== folder && (folder === "." || path.startsWith(`${folder}/`));

/** The unchecked files inside a new folder. Git lists a new folder as one row,
 * so a row that was a file can fold into a folder, and committing the folder
 * would take the file with it. */
export const uncheckedInside = (
  folder: ChangedFile,
  excluded: ReadonlySet<string>,
) =>
  folder.isDirectory
    ? Array.from(excluded).filter((path) => inside(path, folder.path))
    : [];

/** Tests rows against the unchecked paths. A row stays out if it was unchecked,
 * if a folder above it was, or if it's a folder that holds an unchecked file. */
export function leftOutBy(excluded: ReadonlySet<string>) {
  const holding = new Set(Array.from(excluded).flatMap(foldersAbove));
  return (file: ChangedFile) =>
    excluded.has(file.path) ||
    foldersAbove(file.path).some((folder) => excluded.has(folder)) ||
    (file.isDirectory && holding.has(file.path));
}

/** The unchecked paths once a row is checked again. Its unchecked folder gives
 * way to the other rows in that folder, so those stay out. */
export function checkedAgain(
  file: ChangedFile,
  excluded: ReadonlySet<string>,
  files: ChangedFile[],
) {
  const next = new Set(excluded);
  next.delete(file.path);
  for (const path of uncheckedInside(file, excluded)) next.delete(path);
  for (const folder of foldersAbove(file.path))
    if (next.delete(folder))
      for (const row of files)
        if (row !== file && inside(row.path, folder)) next.add(row.path);
  return next;
}

export function useExcludedFiles(workspaceId: string) {
  const excluded = useSyncExternalStore(
    subscribe,
    () => unchecked.get(workspaceId) ?? NONE,
  );
  const setExcluded = useCallback(
    (next: ReadonlySet<string>) => {
      unchecked.set(workspaceId, next);
      listeners.forEach((listener) => listener());
    },
    [workspaceId],
  );
  return [excluded, setExcluded] as const;
}
