import { useRef, type ReactNode } from "react";

/**
 * Load a destination on first visit, then retain its local drafts while hidden.
 * A view that keeps saved records of its own reloads them on each return with
 * useReopenCount; drafts live in separate state and survive that.
 */
export default function RetainedWorkspaceView({
  active,
  children,
}: {
  active: boolean;
  children: ReactNode;
}) {
  const visited = useRef(false);
  if (active) visited.current = true;
  return visited.current ? (
    <div hidden={!active} className="workspace-retained-view">
      {children}
    </div>
  ) : null;
}
