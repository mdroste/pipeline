import { invoke } from "@tauri-apps/api/core";
import type { EngineStatus } from "./engineTypes";

export const engineClient = {
  list: () => invoke<EngineStatus[]>("list_engines"),
  install: (args: { engineId: string }) => invoke<void>("install_engine", args),
  uninstall: (args: { engineId: string }) =>
    invoke<void>("uninstall_engine", args),
  cancelInstall: () => invoke<void>("cancel_engine_install"),
};
