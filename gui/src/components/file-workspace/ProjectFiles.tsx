import { useMemo, useState } from "react";
import FileWorkspace, { type FilePassage } from "./FileWorkspace";
import { workspaceFileAdapter } from "../../lib/fileWorkspaceClient";
import ImportDocumentDialog from "../project-surface/ImportDocumentDialog";
import type { FileLocation } from "../../lib/fileLinks";
import { notify } from "../DialogService";
import { appEvents } from "../../lib/appEvents";
import ScriptRunControls from "../project-surface/ScriptRunControls";
export default function ProjectFiles({
  workspaceId,
  paths,
  initial,
  onAsk,
  onAskFile,
  onSaved,
  onNavigate,
  onRunFile,
  onShowRun,
}: {
  workspaceId: string;
  paths: string[];
  initial?: FileLocation | null;
  onAsk?: (passage: FilePassage) => void;
  onAskFile?: (path: string) => void;
  onSaved?: () => void;
  onNavigate?: (location: FileLocation) => void;
  /** Opens the run setup for a script. */
  onRunFile?: (path: string) => void;
  /** Opens the output of a script's run. */
  onShowRun?: (path: string, runId: string) => void;
}) {
  const [importPath, setImportPath] = useState<string | null>(null);
  const adapter = useMemo(
    () => workspaceFileAdapter({ workspaceId }),
    [workspaceId],
  );
  return (
    <>
      {importPath && (
        <ImportDocumentDialog
          workspaceId={workspaceId}
          path={importPath}
          folder={false}
          onClose={() => setImportPath(null)}
          onImported={async (document) => {
            notify(`Added ${document.paper.title} to Documents`, "success");
            onSaved?.();
          }}
        />
      )}
      <FileWorkspace
        key={adapter.id}
        adapter={adapter}
        paths={paths}
        initial={initial}
        onAsk={onAsk}
        onAskFile={onAskFile}
        runControls={
          onRunFile && onShowRun
            ? ({ path, dirty }) => (
                <ScriptRunControls
                  key={path}
                  workspaceId={workspaceId}
                  path={path}
                  dirty={dirty}
                  onSetup={onRunFile}
                  onShowRun={onShowRun}
                />
              )
            : undefined
        }
        onSaved={onSaved}
        onNavigate={onNavigate}
        onFileDrag={(drag) =>
          appEvents.emit("project-file-drag", { ...drag, workspaceId })
        }
        onAddToDocuments={async (path) => {
          const file = await adapter.read(path);
          if (file.streamToken) adapter.release?.(file.streamToken);
          if (!file.externalPath)
            throw new Error("This file has no local copy");
          setImportPath(file.externalPath);
        }}
      />
    </>
  );
}
