// Files the researcher unchecked, by project, for as long as the app runs. This
// outlives the page on purpose: Changes unmounts for History, and the whole page
// unmounts when the project changes tabs, as Open in Files does. Losing the list
// would put a file the researcher left out back into the next commit.
//
// Paths stay here after they leave the list of changes. A file that disappears
// for a moment, as it can across a branch switch, must come back still unchecked.
import { useCallback, useSyncExternalStore } from "react";

const NONE: ReadonlySet<string> = new Set();
const unchecked = new Map<string, ReadonlySet<string>>();
const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
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
