import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import type {
  UpdateInfo,
  UpdateInstallState,
  UpdateStatus,
} from "../lib/types";

const DISMISS_KEY = "pipeline:update-dismissed-version";
const UPDATE_AVAILABLE_EVENT = "update-available";
const UPDATE_INSTALL_EVENT = "update-install";
const UPDATE_STATUS_EVENT = "update-status";
const IDLE: UpdateInstallState = {
  phase: "idle",
  version: null,
  downloaded: 0,
  total: null,
  error: null,
};

const PRIMARY_BUTTON =
  "shrink-0 px-2.5 py-1 rounded-md bg-inverse text-on-inverse text-xs font-medium hover:opacity-85 transition-opacity";
const QUIET_BUTTON =
  "shrink-0 px-2 py-1 text-xs text-blue-700 dark:text-blue-300 hover:text-blue-900 dark:hover:text-blue-100 transition-colors";

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export default function UpdateBanner({
  confirmRestart,
}: {
  /** Asks before a restart that would interrupt work. */
  confirmRestart: () => Promise<boolean>;
}) {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [install, setInstall] = useState<UpdateInstallState>(IDLE);
  /** Null hides the setting, as in source builds, which never check alone. */
  const [automatic, setAutomatic] = useState<boolean | null>(null);
  const [dismissed, setDismissed] = useState(() =>
    localStorage.getItem(DISMISS_KEY),
  );
  /** "Later" hides the banner until Pipeline next starts. */
  const [postponed, setPostponed] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let heardEvent = false;
    let heardInstall = false;
    let heardPreference = false;
    const stops: (() => void)[] = [];
    const applyPreference = (status: UpdateStatus) =>
      setAutomatic(status.automatic_available ? status.automatic : null);
    const keep = (stop: () => void) => {
      if (cancelled) stop();
      else stops.push(stop);
    };
    const offer = (result: UpdateInfo | null | undefined) => {
      if (cancelled || !result?.update_available) return;
      setInfo(result);
    };
    // The backend checks GitHub on its own schedule; mounting only reads the
    // saved result, after the listeners are active so no event is missed.
    Promise.all([
      listen<UpdateInfo>(UPDATE_AVAILABLE_EVENT, (event) => {
        heardEvent = true;
        offer(event.payload);
      }).then(keep),
      listen<UpdateInstallState>(UPDATE_INSTALL_EVENT, (event) => {
        heardInstall = true;
        if (event.payload.phase !== "idle") setPostponed(false);
        setInstall(event.payload);
      }).then(keep),
      listen<UpdateStatus>(UPDATE_STATUS_EVENT, (event) => {
        heardPreference = true;
        applyPreference(event.payload);
      }).then(keep),
    ])
      .catch(() => {})
      .then(() => {
        if (cancelled) return;
        void invoke<UpdateInstallState>("update_install_state")
          .then((saved) => {
            if (!cancelled && !heardInstall && saved) setInstall(saved);
          })
          .catch(() => {});
        return invoke<UpdateStatus>("update_status");
      })
      .then((status) => {
        if (cancelled || !status) return;
        if (!heardPreference) applyPreference(status);
        // A live event is never replaced by an older saved result.
        if (!heardEvent) offer(status.info);
      })
      .catch(() => {
        // Silent on failure — don't nag the user offline.
      });
    return () => {
      cancelled = true;
      for (const stop of stops) stop();
    };
  }, []);

  const offered =
    info !== null && info.latest !== dismissed && !postponed ? info : null;
  if (install.phase === "idle" && !offered) return null;

  const openPage = async (url: string) => {
    setActionError(null);
    try {
      await openUrl(url);
    } catch (error) {
      setActionError(`Could not open the release page: ${message(error)}`);
    }
  };

  const runCommand = async (command: string) => {
    setActionError(null);
    try {
      await invoke(command);
    } catch (error) {
      setActionError(message(error));
    }
  };

  const startInstall = () => runCommand("update_install_start");
  const stopInstall = () => runCommand("update_install_cancel");

  const restart = async () => {
    if (await confirmRestart()) await runCommand("update_install_restart");
  };

  const later = async () => {
    setPostponed(true);
    await stopInstall();
  };

  const dismiss = () => {
    if (info) {
      localStorage.setItem(DISMISS_KEY, info.latest);
      setDismissed(info.latest);
    }
    void stopInstall();
  };

  const saveAutomatic = async (enabled: boolean) => {
    setActionError(null);
    try {
      await invoke("update_set_automatic", { enabled });
    } catch {
      setActionError("Could not save the update setting. Try again.");
    }
  };

  const version = install.version ?? info?.latest ?? "";
  const percent =
    install.total && install.total > 0
      ? Math.min(100, Math.round((install.downloaded / install.total) * 100))
      : null;
  const browserDownload = info ? (info.download_url ?? info.release_url) : null;

  let summary: ReactNode;
  let actions: ReactNode;
  if (install.phase === "downloading") {
    summary = (
      <>
        <span className="font-medium">Downloading version {version}…</span>
        {percent !== null && <span aria-hidden="true"> {percent}%</span>}
      </>
    );
    actions = (
      <>
        {percent !== null && (
          <div
            role="progressbar"
            aria-label="Update download"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent}
            className="w-32 h-1.5 shrink-0 overflow-hidden rounded-full bg-blue-200 dark:bg-blue-900"
          >
            <div
              className="h-full rounded-full bg-blue-600 transition-[width]"
              style={{ width: `${percent}%` }}
            />
          </div>
        )}
        <button onClick={() => void stopInstall()} className={QUIET_BUTTON}>
          Cancel
        </button>
      </>
    );
  } else if (install.phase === "ready") {
    summary = (
      <>
        <span className="font-medium">Version {version} is ready.</span>{" "}
        Pipeline will restart to finish installing it.
      </>
    );
    actions = (
      <>
        <button onClick={() => void restart()} className={PRIMARY_BUTTON}>
          Restart now
        </button>
        <button onClick={() => void later()} className={QUIET_BUTTON}>
          Later
        </button>
      </>
    );
  } else if (install.phase === "installing") {
    summary = (
      <span className="font-medium">
        Installing version {version}… Pipeline will restart.
      </span>
    );
  } else if (install.phase === "failed") {
    summary = (
      <span role="alert">
        <span className="font-medium">Update failed.</span> {install.error}
      </span>
    );
    actions = (
      <>
        <button onClick={() => void startInstall()} className={PRIMARY_BUTTON}>
          Try again
        </button>
        {browserDownload && (
          <button
            onClick={() => void openPage(browserDownload)}
            className={QUIET_BUTTON}
          >
            Download instead
          </button>
        )}
        <button onClick={dismiss} className={QUIET_BUTTON}>
          Dismiss
        </button>
      </>
    );
  } else if (offered) {
    summary = (
      <>
        <span className="font-medium">Update available:</span> v
        {offered.current} &rarr; v{offered.latest}
      </>
    );
    actions = (
      <>
        {offered.can_install ? (
          <button
            onClick={() => void startInstall()}
            className={PRIMARY_BUTTON}
          >
            Install update
          </button>
        ) : (
          <button
            onClick={() =>
              void openPage(offered.download_url ?? offered.release_url)
            }
            className={PRIMARY_BUTTON}
          >
            Download
          </button>
        )}
        <button
          onClick={() => void openPage(offered.release_url)}
          className={QUIET_BUTTON}
        >
          What’s new
        </button>
        {automatic !== null && (
          <label className="shrink-0 flex items-center gap-1.5 text-xs text-blue-700 dark:text-blue-300">
            <input
              type="checkbox"
              checked={automatic}
              onChange={(event) => void saveAutomatic(event.target.checked)}
            />
            Check automatically
          </label>
        )}
        <button onClick={dismiss} className={QUIET_BUTTON}>
          Dismiss
        </button>
      </>
    );
  }

  return (
    <div
      role="status"
      className="flex items-center gap-3 px-6 py-2 bg-blue-50 dark:bg-blue-950 border-b border-blue-200 dark:border-blue-900 text-sm text-blue-800 dark:text-blue-200"
    >
      <svg
        className="w-4 h-4 shrink-0"
        viewBox="0 0 20 20"
        fill="currentColor"
        aria-hidden="true"
      >
        <path
          fillRule="evenodd"
          d="M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0Zm-7-4a1 1 0 1 1-2 0 1 1 0 0 1 2 0ZM9 9a1 1 0 0 0 0 2v3a1 1 0 0 0 1 1h1a1 1 0 1 0 0-2v-3a1 1 0 0 0-1-1H9Z"
          clipRule="evenodd"
        />
      </svg>
      {/* A failure explains itself, so it wraps rather than being cut off. */}
      <span
        className={`flex-1 min-w-0 ${install.phase === "failed" ? "" : "truncate"}`}
      >
        {summary}
      </span>
      {actionError && (
        <span
          role="alert"
          className="min-w-0 truncate text-xs text-red-700 dark:text-red-300"
        >
          {actionError}
        </span>
      )}
      {actions}
    </div>
  );
}
