import { invoke } from "@tauri-apps/api/core";

export interface ToolSetupStatus {
  available: boolean;
  reason: string | null;
}
export const toolSetupClient = {
  status() {
    return invoke<ToolSetupStatus>("workbench_shell_status");
  },
};
