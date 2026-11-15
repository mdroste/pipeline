import { invoke } from "@tauri-apps/api/core";
import type {
  UpdateInfo,
  UpdateInstallState,
  UpdateStatus,
} from "./updateTypes";

export const updateClient = {
  status: () => invoke<UpdateStatus>("update_status"),
  check: () => invoke<UpdateInfo>("check_for_update"),
  installState: () => invoke<UpdateInstallState>("update_install_state"),
  startInstall: () => invoke<void>("update_install_start"),
  cancelInstall: () => invoke<void>("update_install_cancel"),
  restart: () => invoke<void>("update_install_restart"),
  setAutomatic: (args: { enabled: boolean }) =>
    invoke<UpdateStatus>("update_set_automatic", args),
};
