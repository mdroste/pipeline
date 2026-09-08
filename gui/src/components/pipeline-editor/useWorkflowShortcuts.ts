import { useEffect, useRef } from "react";

import type { WorkflowEditor } from "./useWorkflowEditor";
import type { ProfileOperations } from "./useProfileOperations";
export function useWorkflowShortcuts(
  editor: WorkflowEditor,
  profiles: ProfileOperations,
) {
  const {
    dirty,
    schemaDraftValid,
    history: { undo, redo },
  } = editor;
  const { saving, handleSave } = profiles;
  const saveShortcutRef = useRef<() => void>(() => {});
  const undoShortcutRef = useRef<() => void>(() => {});
  const redoShortcutRef = useRef<() => void>(() => {});
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      const key = event.key.toLowerCase();
      if (key === "s") {
        event.preventDefault();
        saveShortcutRef.current();
      } else if (key === "z" && event.shiftKey) {
        event.preventDefault();
        redoShortcutRef.current();
      } else if (key === "z") {
        event.preventDefault();
        undoShortcutRef.current();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  saveShortcutRef.current = () => {
    if (dirty && schemaDraftValid && !saving) void handleSave();
  };
  undoShortcutRef.current = undo;
  redoShortcutRef.current = redo;
}
