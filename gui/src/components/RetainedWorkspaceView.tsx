import { useRef, type ReactNode } from "react";

/** Load a destination on first visit, then retain its local drafts while hidden. */
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
