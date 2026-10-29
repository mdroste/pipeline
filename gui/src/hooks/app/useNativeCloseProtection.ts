import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirmDialog } from "../../components/DialogService";
import type { AppNavigation } from "./useAppNavigation";

export function useNativeCloseProtection(
  unsavedRef: AppNavigation["unsavedRef"],
) {
  const [closeProtectionUnavailable, setCloseProtectionUnavailable] =
    useState(false);
  const allowCloseRef = useRef(false);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onCloseRequested(async (event) => {
        if (allowCloseRef.current) return;
        const unsaved = unsavedRef.current;
        if (
          !unsaved.workflowDirty &&
          !unsaved.settingsDirty &&
          !unsaved.projectsDirty &&
          !unsaved.tasksDirty
        )
          return;
        event.preventDefault();
        if (
          await confirmDialog(
            "You have unsaved changes. Quit and discard them?",
            {
              title: "Quit Pipeline?",
              confirmLabel: "Discard and quit",
              destructive: true,
            },
          )
        ) {
          allowCloseRef.current = true;
          await getCurrentWindow().close();
        }
      })
      .then((stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      })
      .catch((error) => {
        console.warn("Unable to register close protection:", error);
        if (!disposed) setCloseProtectionUnavailable(true);
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return { closeProtectionUnavailable, setCloseProtectionUnavailable };
}
