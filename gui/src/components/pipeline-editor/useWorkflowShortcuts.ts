import { useEffect, useRef } from "react";

import type { WorkflowEditor } from "./useWorkflowEditor";
import type { ProfileOperations } from "./useProfileOperations";

// Text fields keep the browser's own undo; workflow undo applies elsewhere.
function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

// Dialogs opened over the editor own the keyboard while they are open.
function modalOpen(): boolean {
  return document.querySelector('[aria-modal="true"], dialog[open]') !== null;
}

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
      if (event.defaultPrevented || event.isComposing || modalOpen()) return;
      const key = event.key.toLowerCase();
      if (key === "s") {
        event.preventDefault();
        saveShortcutRef.current();
        return;
      }
      if (key !== "z" || isEditableTarget(event.target)) return;
      event.preventDefault();
      if (event.shiftKey) redoShortcutRef.current();
      else undoShortcutRef.current();
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
