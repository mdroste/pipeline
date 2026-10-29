import { useCallback, useEffect, useState } from "react";
import type { DeskLayout } from "../../lib/deskLayout";
import type {
  Annotation,
  DocumentSelection,
  ProjectRecord,
} from "../../lib/projectClient";
import type { ProjectTab } from "./shared";

export function useProjectReadingState(
  workspaceId: string,
  layout: DeskLayout,
  setTab: (tab: ProjectTab) => void,
) {
  const [revisionId, setRevisionId] = useState<string | null>(
    layout.revisionId,
  );
  const [pinnedRevision, setPinnedRevision] = useState<string | null>(() => {
    try {
      return (
        layout.pinnedRevision ??
        localStorage.getItem(`pipeline.project.${workspaceId}.pinned`)
      );
    } catch {
      return null;
    }
  });
  useEffect(() => {
    try {
      if (pinnedRevision)
        localStorage.setItem(
          `pipeline.project.${workspaceId}.pinned`,
          pinnedRevision,
        );
      else localStorage.removeItem(`pipeline.project.${workspaceId}.pinned`);
    } catch {
      /* Layout storage is optional. */
    }
  }, [workspaceId, pinnedRevision]);
  const [initialSelection, setInitialSelection] = useState<
    DocumentSelection | undefined
  >();
  const [pinnedSelection, setPinnedSelection] = useState<
    DocumentSelection | undefined
  >();
  const openAnnotation = useCallback(
    (annotation: ProjectRecord<Annotation>) => {
      setRevisionId(annotation.body.selection.revisionId);
      setInitialSelection(annotation.body.selection);
      setTab("documents");
    },
    [],
  );
  return {
    revisionId,
    setRevisionId,
    pinnedRevision,
    setPinnedRevision,
    initialSelection,
    setInitialSelection,
    pinnedSelection,
    setPinnedSelection,
    openAnnotation,
  };
}
export type ProjectReadingState = ReturnType<typeof useProjectReadingState>;
