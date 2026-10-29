import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { UpdateInfo, UpdateStatus } from "../lib/types";

const DISMISS_KEY = "pipeline:update-dismissed-version";
const UPDATE_AVAILABLE_EVENT = "update-available";

export default function UpdateBanner() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let heardEvent = false;
    let unlisten: (() => void) | null = null;
    const offer = (result: UpdateInfo | null | undefined) => {
      if (cancelled || !result?.update_available) return;
      // Respect prior dismissal for this specific version.
      if (localStorage.getItem(DISMISS_KEY) === result.latest) return;
      setInfo(result);
    };
    // The backend checks GitHub on its own schedule; mounting only reads the
    // saved result, after the listener is active so no event is missed.
    listen<UpdateInfo>(UPDATE_AVAILABLE_EVENT, (event) => {
      heardEvent = true;
      offer(event.payload);
    })
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch(() => {})
      .then(() => (cancelled ? null : invoke<UpdateStatus>("update_status")))
      .then((status) => {
        // A live event is never replaced by an older saved result.
        if (!heardEvent) offer(status?.info);
      })
      .catch(() => {
        // Silent on failure — don't nag the user offline.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (!info) return null;

  const dismiss = () => {
    localStorage.setItem(DISMISS_KEY, info.latest);
    setInfo(null);
  };

  const openPage = async (url: string) => {
    setOpenError(null);
    try {
      await openUrl(url);
    } catch (error) {
      setOpenError(error instanceof Error ? error.message : String(error));
    }
  };

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
      <span className="flex-1 min-w-0 truncate">
        <span className="font-medium">Update available:</span> v{info.current}{" "}
        &rarr; v{info.latest}
      </span>
      {openError && (
        <span
          role="alert"
          className="min-w-0 truncate text-xs text-red-700 dark:text-red-300"
        >
          Could not open the release page: {openError}
        </span>
      )}
      <button
        onClick={() => void openPage(info.download_url ?? info.release_url)}
        className="shrink-0 px-2.5 py-1 rounded-md bg-blue-600 text-white text-xs font-medium hover:bg-blue-700 transition-colors"
      >
        Download
      </button>
      <button
        onClick={() => void openPage(info.release_url)}
        className="shrink-0 px-2 py-1 text-xs text-blue-700 dark:text-blue-300 hover:text-blue-900 dark:hover:text-blue-100 transition-colors"
      >
        What’s new
      </button>
      <button
        onClick={dismiss}
        className="shrink-0 px-2 py-1 text-xs text-blue-700 dark:text-blue-300 hover:text-blue-900 dark:hover:text-blue-100 transition-colors"
        aria-label="Dismiss"
      >
        Dismiss
      </button>
    </div>
  );
}
