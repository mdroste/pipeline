import { useEffect, useRef } from "react";
import type { AppRoute } from "../lib/router";
export type WorkspaceEntryTarget = Extract<AppRoute, { page: "workspace" }>;

/** One explicit route transition; superseded asynchronous loads never commit. */
export function useWorkspaceRouteEntry(
  request: number,
  target: WorkspaceEntryTarget | undefined,
  apply: (
    target: WorkspaceEntryTarget,
    current: () => boolean,
  ) => Promise<void>,
  onError: (error: unknown) => void,
) {
  const latest = useRef({ apply, onError });
  latest.current = { apply, onError };
  useEffect(() => {
    if (!target) return;
    let live = true;
    void latest.current
      .apply(target, () => live)
      .catch((error) => {
        if (live) latest.current.onError(error);
      });
    return () => {
      live = false;
    };
  }, [request, target]);
}
