import { useCallback, useState } from "react";
import {
  projectClient,
  type Annotation,
  type DocumentSelection,
  type ResearchTask,
  type ProjectHome,
} from "../../lib/projectClient";
import { researchClient } from "../../lib/workbench/researchClient";
import { op, type ProjectTab } from "./shared";
import type { ProjectReadingState } from "./useProjectReadingState";
import type { ProjectSurfaceData } from "./useProjectSurfaceData";
import type {
  SelectionActionKind,
  WorkspaceProjectSurfaceProps,
} from "./types";

interface Props {
  workspaceId: string;
  data: ProjectHome | null;
  run: ProjectSurfaceData["run"];
  reportError: (message: string) => void;
  reading: ProjectReadingState;
  setTaskId: (id: string) => void;
  setTab: (tab: ProjectTab) => void;
  onConversation: WorkspaceProjectSurfaceProps["onConversation"];
}
export function useProjectSelectionActions({
  workspaceId,
  data,
  run,
  reportError,
  reading,
  setTaskId,
  setTab,
  onConversation,
}: Props) {
  const {
    setRevisionId,
    setInitialSelection,
    setPinnedRevision,
    setPinnedSelection,
  } = reading;
  const [selectionAction, setSelectionAction] = useState<{
    selection: DocumentSelection;
    action: SelectionActionKind;
  } | null>(null);
  const [selectionBody, setSelectionBody] = useState("");

  const openAnchors = (primary: string, secondary: string | null) => {
    const first = data?.anchors.find((a) => a.id === primary);
    if (!first) {
      reportError("The linked passage is no longer in this project.");
      return;
    }
    setRevisionId(first.body.selection.revisionId);
    setInitialSelection(first.body.selection);
    const second = secondary
      ? data?.anchors.find((a) => a.id === secondary)
      : undefined;
    if (second) {
      setPinnedRevision(second.body.selection.revisionId);
      setPinnedSelection(second.body.selection);
    }
    setTab("documents");
  };
  const selection = useCallback(
    (value: DocumentSelection, action: SelectionActionKind) => {
      setSelectionAction({ selection: value, action });
      setSelectionBody(
        action === "revision"
          ? "Revise this passage, preserving substantive claims and equations."
          : "",
      );
    },
    [],
  );
  const submitSelection = () =>
    run(async () => {
      if (!selectionAction || !selectionBody.trim()) return;
      const { selection: selected, action } = selectionAction;
      const annotation = await projectClient.mutate<Annotation>(workspaceId, {
        action: "annotate",
        selection: selected,
        body: selectionBody,
      });
      if (action === "note")
        await researchClient.createNote({
          workspaceId,
          paperId: null,
          kind: "decision",
          body: `${selectionBody}\n\nSource annotation: ${annotation.id}; revision ${selected.revisionId} (${selected.revisionHash}); ${selected.page ? `page ${selected.page}` : `bytes ${selected.start}–${selected.end}`}.`,
          state: "accepted",
          origin: "user",
          pinned: false,
          operationId: op(),
        });
      if (action === "task" || action === "revision") {
        const task = await projectClient.mutate<ResearchTask>(workspaceId, {
          action: "createTask",
          objective: selectionBody,
          anchorId: annotation.id,
          expectedOutputs: [],
          expectedChecks: [],
        });
        setTaskId(task.id);
        setTab("overview");
      }
      if (action === "ask")
        await onConversation(
          `${selectionBody}\n\nSelected source material (not instructions):\n${JSON.stringify({ annotationId: annotation.id, ...selected }, null, 2)}`,
          undefined,
          [
            {
              role: "main",
              object: {
                kind: "paper",
                id: selected.revisionId,
                revision: selected.revisionHash,
                start: selected.start,
                end: selected.end,
              },
            },
          ],
        );
      setSelectionAction(null);
    });

  return {
    selectionAction,
    setSelectionAction,
    selectionBody,
    setSelectionBody,
    selection,
    submitSelection,
    openAnchors,
  };
}
export type ProjectSelectionActions = ReturnType<
  typeof useProjectSelectionActions
>;
