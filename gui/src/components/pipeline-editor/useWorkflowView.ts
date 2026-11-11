import { useState } from "react";
import type { WorkflowLintWarning } from "../../lib/pipelineHelpers";

export function useWorkflowView() {
  const [addStepOpen, setAddStepOpen] = useState(false);
  const [warningTarget, setWarningTarget] = useState<
    (WorkflowLintWarning & { request: number }) | null
  >(null);
  const [navigatorView, setNavigatorView] = useState<
    "steps" | "overview" | "schemas"
  >("steps");
  const [editorMode, setEditorMode] = useState<"basic" | "advanced">("basic");
  const [catalogTab, setCatalogTab] = useState<"subjects" | "methods" | null>(
    null,
  );
  return {
    warningTarget,
    setWarningTarget,
    addStepOpen,
    setAddStepOpen,
    navigatorView,
    setNavigatorView,
    editorMode,
    setEditorMode,
    catalogTab,
    setCatalogTab,
  };
}
export type WorkflowView = ReturnType<typeof useWorkflowView>;
