import { useMemo } from "react";
import FileWorkspace, { type FilePassage } from "./FileWorkspace";
import { workspaceFileAdapter } from "../../lib/fileWorkspaceClient";
import type { FileLocation } from "../../lib/fileLinks";
export default function ProjectFiles({
  workspaceId,
  paths,
  initial,
  onAsk,
  onSaved,
}: {
  workspaceId: string;
  paths: string[];
  initial?: FileLocation | null;
  onAsk?: (passage: FilePassage) => void;
  onSaved?: () => void;
}) {
  const adapter = useMemo(
    () => workspaceFileAdapter({ workspaceId }),
    [workspaceId],
  );
  return (
    <FileWorkspace
      key={adapter.id}
      adapter={adapter}
      paths={paths}
      initial={initial}
      onAsk={onAsk}
      onSaved={onSaved}
    />
  );
}
