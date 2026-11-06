import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { confirmDialog, notify } from "../../components/DialogService";
import type { AppNavigation } from "./useAppNavigation";
import { flushDrafts } from "../../lib/flushDrafts";

export function useNativeCloseProtection(
  unsavedRef: AppNavigation["unsavedRef"],
) {
  const [closeProtectionUnavailable, setCloseProtectionUnavailable] =
    useState(false);
  useEffect(() => {
    let disposed = false;
    let pending = false;
    const id = `${Date.now()}-${Math.random()}`;
    const unlisten: (() => void)[] = [];
    const requestClose = async (quit: boolean) => {
      if (disposed || pending) return;
      pending = true;
      try {
        const hide = !quit && (await invoke<boolean>("app_close_hides"));
        if (disposed) return;
        const unsaved = unsavedRef.current;
        if (
          !hide &&
          (unsaved.workflowDirty ||
            unsaved.settingsDirty ||
            unsaved.projectsDirty ||
            unsaved.tasksDirty) &&
          !(await confirmDialog(
            "You have unsaved changes. Quit and discard them?",
            {
              title: "Quit Pipeline?",
              confirmLabel: "Discard and quit",
              destructive: true,
            },
          ))
        )
          return;
        if (!disposed) {
          if (!hide) flushDrafts();
          await invoke("complete_app_close", {
            action: hide ? "hide" : "quit",
          });
        }
      } catch (error) {
        if (!disposed) notify(`Could not close Pipeline: ${String(error)}`);
      } finally {
        pending = false;
      }
    };
    const register = async () => {
      const keep = (stop: () => void) => {
        if (disposed) stop();
        else unlisten.push(stop);
      };
      try {
        keep(
          await getCurrentWindow().onCloseRequested((event) => {
            // Tauri's JS API otherwise destroys the window after this callback,
            // even when a native close handler has prevented closing.
            event.preventDefault();
            return requestClose(false);
          }),
        );
        if (disposed) return;
        keep(await listen("app:quit-requested", () => requestClose(true)));
        if (disposed) return;
        await invoke("set_app_close_guard", { id, active: true });
        if (disposed)
          await invoke("set_app_close_guard", { id, active: false });
      } catch (error) {
        console.warn("Unable to register close protection:", error);
        if (!disposed) setCloseProtectionUnavailable(true);
      }
    };
    void register();
    return () => {
      disposed = true;
      unlisten.forEach((stop) => stop());
      void invoke("set_app_close_guard", { id, active: false }).catch(
        console.warn,
      );
    };
  }, []);

  return { closeProtectionUnavailable, setCloseProtectionUnavailable };
}
