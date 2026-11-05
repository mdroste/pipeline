import { artifactClient } from "../../lib/artifactClient";
import { useMemo, type ReactNode } from "react";
import { FileNavigationContext } from "./FileNavigation";
import { artifactFileAdapter } from "../../lib/fileWorkspaceClient";
import type { FileLocation } from "../../lib/fileLinks";
export default function RunMarkdown({
  runId,
  path = "report.md",
  onOpen,
  children,
}: {
  runId: string;
  path?: string;
  onOpen: (location: FileLocation) => void;
  children: ReactNode;
}) {
  const navigation = useMemo(() => {
    const adapter = async () => {
      const manifest = await artifactClient.manifest(runId);
      return artifactFileAdapter(
        runId,
        manifest.artifacts.map((a) => a.rel_path),
      );
    };
    return {
      path,
      open: async (location: FileLocation) => {
        await (await adapter()).read(location.path);
        onOpen(location);
      },
      image: async (path: string) => {
        const file = await (await adapter()).read(path);
        if (!file.base64 || !file.mime.startsWith("image/"))
          throw new Error("Image not retained in this run");
        return `data:${file.mime};base64,${file.base64}`;
      },
    };
  }, [runId, path, onOpen]);
  return (
    <FileNavigationContext.Provider value={navigation}>
      {children}
    </FileNavigationContext.Provider>
  );
}
