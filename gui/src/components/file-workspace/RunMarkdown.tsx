import { artifactClient } from "../../lib/artifactClient";
import { useMemo, useRef, type ReactNode } from "react";
import { FileNavigationContext } from "./FileNavigation";
import {
  readArtifactImage,
  UNRETAINED_ARTIFACT,
} from "../../lib/fileWorkspaceClient";
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
  // Callers pass inline handlers. Reading the latest one through a ref keeps
  // the navigation value stable, so a parent re-render does not reset every
  // image in the document and read it again.
  const openRef = useRef(onOpen);
  openRef.current = onOpen;
  const navigation = useMemo(() => {
    // One manifest read serves every link and image in this document. A miss
    // reads it again, so an artifact retained after the first read still opens.
    let retained: Promise<Set<string>> | null = null;
    const manifestPaths = (fresh: boolean): Promise<Set<string>> => {
      if (!fresh && retained) return retained;
      const read = artifactClient
        .manifest(runId)
        .then((manifest) => new Set(manifest.artifacts.map((a) => a.rel_path)));
      retained = read;
      // A failed read is not kept, so the next image tries again.
      read.catch(() => {
        if (retained === read) retained = null;
      });
      return read;
    };
    const requireRetained = async (target: string) => {
      if ((await manifestPaths(false)).has(target)) return;
      if (!(await manifestPaths(true)).has(target))
        throw new Error(UNRETAINED_ARTIFACT);
    };
    return {
      path,
      open: async (location: FileLocation) => {
        await requireRetained(location.path);
        await artifactClient.read(runId, location.path);
        openRef.current(location);
      },
      image: async (target: string) => {
        await requireRetained(target);
        return readArtifactImage(runId, target);
      },
    };
  }, [runId, path]);
  return (
    <FileNavigationContext.Provider value={navigation}>
      {children}
    </FileNavigationContext.Provider>
  );
}
