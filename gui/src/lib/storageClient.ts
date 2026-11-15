import { invoke } from "@tauri-apps/api/core";

export interface StorageLocation {
  activeDirectory: string | null;
  configuredDirectory: string;
  defaultDirectory: string;
  restartRequired: boolean;
  error: string | null;
}

export const storageClient = {
  location: () => invoke<StorageLocation>("get_storage_settings"),
  setDirectory: (args: { directory: string }) =>
    invoke<StorageLocation>("set_storage_directory", args),
  openDirectory: () => invoke<void>("open_pipeline_dir"),
};
