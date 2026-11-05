import type { Mock } from "vitest";
import type { Settings, EngineStatus } from "../lib/types";

export function mockSettingsLoad(
  invoke: Pick<Mock, "mockImplementation">,
  settings: Settings,
  warnings: string[] = [],
  engines: EngineStatus[] = [],
) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_settings") return Promise.resolve({ settings, warnings });
    if (cmd === "get_storage_settings")
      return Promise.resolve({
        activeDirectory: "/home/researcher/.pipeline",
        configuredDirectory: "/home/researcher/.pipeline",
        defaultDirectory: "/home/researcher/.pipeline",
        restartRequired: false,
        error: null,
      });
    if (cmd === "workbench_codex_connect") return Promise.resolve({});
    if (cmd === "workbench_codex_account_state")
      return Promise.resolve({ status: "signedOut" });
    if (cmd === "save_settings") return Promise.resolve();
    if (cmd === "list_engines") return Promise.resolve(engines);
    if (cmd === "workflow_codex_status" || cmd === "chatgpt_account_status")
      return Promise.resolve({
        account: { status: "signedOut", email: null, planType: null },
        version: "0.153.4",
        epoch: 1,
        loginInProgress: false,
        unresolvedAttempts: [],
      });
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}
