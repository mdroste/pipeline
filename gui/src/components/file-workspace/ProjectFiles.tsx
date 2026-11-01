import { useMemo } from "react";
import FileWorkspace, { type FilePassage } from "./FileWorkspace";
import { workspaceFileAdapter } from "../../lib/fileWorkspaceClient";
import { importResearchFiles } from "../../lib/workspaceAttachments";
import type { FileLocation } from "../../lib/fileLinks";
import { notify } from "../DialogService";
export default function ProjectFiles({
  workspaceId,
  paths,
  initial,
  onAsk,
  onAskFile,
  onSaved,
}: {
  workspaceId: string;
  paths: string[];
  initial?: FileLocation | null;
  onAsk?: (passage: FilePassage) => void;
  onAskFile?: (path: string) => void;
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
      onAskFile={onAskFile}
      onSaved={onSaved}
      onAddToDocuments={async (path) => {
        const file = await adapter.read(path);
        if (file.streamToken) adapter.release?.(file.streamToken);
        if (!file.externalPath) throw new Error("This file has no local copy");
        const { failures } = await importResearchFiles(workspaceId, [
          file.externalPath,
        ]);
        if (failures.length) throw new Error(failures.join("; "));
        notify(`Added ${path} to Documents`, "success");
        onSaved?.();
      }}
    />
  );
}
