export interface UpdateInfo {
  current: string;
  latest: string;
  update_available: boolean;
  release_url: string;
  release_name: string;
  published_at: string | null;
  /** This platform's installer, when the release includes one. */
  download_url: string | null;
  /** Whether this copy can install the release itself. */
  can_install: boolean;
  checked_at: string | null;
}

/** Progress of an in-app update (`update_install_state`, `update-install`). */
export interface UpdateInstallState {
  phase: "idle" | "downloading" | "ready" | "installing" | "failed";
  version: string | null;
  downloaded: number;
  total: number | null;
  error: string | null;
}

/** Saved update preference and last result (`update_status`). */
export interface UpdateStatus {
  automatic: boolean;
  /** False for development and source builds, which check only on request. */
  automatic_available: boolean;
  last_checked_at: string | null;
  info: UpdateInfo | null;
  error: string | null;
}
