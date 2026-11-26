import { invoke } from "@tauri-apps/api/core";

export interface CliToolsStatus {
  installed: boolean;
  available: boolean;
  reason: string | null;
  path: string;
  command: string;
  terminal_setup: string;
}

export const cliToolsClient = {
  status: () => invoke<CliToolsStatus>("cli_tools_status"),
  install: () => invoke<CliToolsStatus>("install_cli_tools"),
  uninstall: () => invoke<CliToolsStatus>("uninstall_cli_tools"),
};
