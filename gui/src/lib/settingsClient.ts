import { invoke } from "@tauri-apps/api/core";
import type { Settings, ModelCatalog } from "./types";
export interface SettingsResponse {
  settings: Settings;
  warnings: string[];
  undecryptable_keys?: string[];
}

export const settingsClient = {
  load: () => invoke<SettingsResponse>("get_settings"),
  save: (args: { settings: Settings }) =>
    invoke<unknown>("save_settings", args),
  discardSecret: (args: { provider: string }) =>
    invoke<unknown>("discard_undecryptable_secret", args),
  modelCatalog: (args: {
    provider: string;
    settings: Settings;
    refresh: boolean;
  }) => invoke<ModelCatalog>("get_model_catalog", args),
};
