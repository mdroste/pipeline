import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { UpdateInfo } from "../lib/types";

const DISMISS_KEY = "pipeline:update-dismissed-version";

export default function UpdateBanner() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<UpdateInfo>("check_for_update")
      .then((result) => {
        if (cancelled) return;
        if (!result.update_available) return;
        // Respect prior dismissal for this specific version.
        if (localStorage.getItem(DISMISS_KEY) === result.latest) return;
        setInfo(result);
      })
      .catch(() => {
        // Silent on network failure — don't nag the user offline.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!info) return null;

  const dismiss = () => {
    localStorage.setItem(DISMISS_KEY, info.latest);
    setInfo(null);
  };

  const viewRelease = async () => {
    setOpenError(null);
    try {
      await openUrl(info.release_url);
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
        onClick={() => void viewRelease()}
        className="shrink-0 px-2.5 py-1 rounded-md bg-blue-600 text-white text-xs font-medium hover:bg-blue-700 transition-colors"
      >
        View release
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
