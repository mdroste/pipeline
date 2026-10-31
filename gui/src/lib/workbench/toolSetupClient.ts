import { invoke } from "@tauri-apps/api/core";

export type ToolSetupKind = "browser" | "shell";
export interface ToolSetupStatus {
  installed: boolean;
  platformAvailable?: boolean;
  setupPhase?: string | null;
}
export const toolSetupClient = {
  status(kind: ToolSetupKind) {
    return invoke<ToolSetupStatus>(`workbench_${kind}_status`);
  },
  install(kind: ToolSetupKind) {
    return invoke<ToolSetupStatus>(`workbench_${kind}_install`);
  },
  cancel() {
    return invoke<void>("workbench_tool_setup_cancel");
  },
};
