/** Mirrors engines::EngineStatus. */
export interface EngineDownloadProgress {
  label: string;
  received_bytes: number;
  total_bytes: number | null;
  bytes_per_second: number;
  retrying: boolean;
}

export interface EngineInstallProgress {
  engine_id: string;
  phases: Record<string, string>;
  log_lines: string[];
  download?: EngineDownloadProgress | null;
}

export interface EngineStatus {
  id: string;
  label: string;
  description: string;
  installed: boolean;
  version: string;
  entry_path: string;
  system_path: string;
  est_download_mb: number;
  est_disk_mb: number;
  managed_stack_mb: number;
  installing: boolean;
  setup_message?: string | null;
  install_progress?: EngineInstallProgress | null;
  available?: boolean;
  unavailable_reason?: string;
}
