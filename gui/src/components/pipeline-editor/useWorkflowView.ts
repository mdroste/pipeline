import { useState } from "react";

export function useWorkflowView() {
  const [addStepOpen, setAddStepOpen] = useState(false);
  const [navigatorView, setNavigatorView] = useState<
    "steps" | "overview" | "schemas"
  >("steps");
  const [editorMode, setEditorMode] = useState<"basic" | "advanced">("basic");
  const [catalogTab, setCatalogTab] = useState<"subjects" | "methods" | null>(
    null,
  );
  return {
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
