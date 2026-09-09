import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface StorageLocation {
  activeDirectory: string | null;
  configuredDirectory: string;
  defaultDirectory: string;
  restartRequired: boolean;
  error: string | null;
}

const buttonClass =
  "rounded-lg border border-gray-300 px-3 py-2 text-xs font-medium hover:bg-gray-50 disabled:opacity-40 dark:border-neutral-600 dark:hover:bg-neutral-800";

export default function StorageSettings({
  onSavingChange,
}: {
  onSavingChange: (saving: boolean) => void;
}) {
  const [location, setLocation] = useState<StorageLocation | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let mounted = true;
    invoke<StorageLocation>("get_storage_settings")
      .then((result) => {
        if (mounted) setLocation(result);
      })
      .catch((reason) => {
        if (mounted) setError(String(reason));
      });
    return () => {
      mounted = false;
    };
  }, []);

  async function changeFolder(useDefault = false) {
    if (!location || busy) return;
    setBusy(true);
    setError(null);
    onSavingChange(true);
    try {
      const directory = useDefault
        ? location.defaultDirectory
        : await open({
            title: "Choose Pipeline research data folder",
            directory: true,
            multiple: false,
            defaultPath: location.configuredDirectory,
          });
      if (typeof directory === "string") {
        setLocation(
          await invoke<StorageLocation>("set_storage_directory", { directory }),
        );
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
      onSavingChange(false);
    }
  }

  return (
    <section
      id="general-storage"
      tabIndex={-1}
      className="settings-card settings-anchor"
      aria-labelledby="storage-heading"
    >
      <h2 id="storage-heading" className="text-base font-semibold">
        Research data folder{" "}
        <span className="settings-status-badge ml-2">Requires restart</span>
      </h2>
      <p className="mt-1 text-sm text-gray-500 dark:text-neutral-400">
        Save project conversations and documents, reviews, automations, and
        activity records here. Sign-ins, app preferences, installed tools, and
        caches stay on this computer.
      </p>
      {location ? (
        <div className="mt-5 space-y-3">
          <label
            className="block text-xs font-medium"
            htmlFor="research-data-folder"
          >
            {location.restartRequired
              ? "Folder after restart"
              : "Current folder"}
          </label>
          <input
            id="research-data-folder"
            readOnly
            value={location.configuredDirectory}
            className="w-full rounded-lg border border-gray-300 bg-gray-50 px-3 py-2 text-sm dark:border-neutral-600 dark:bg-neutral-800"
          />
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              className={buttonClass}
              disabled={busy}
              onClick={() => void changeFolder()}
            >
              {busy ? "Saving…" : "Choose folder…"}
            </button>
            <button
              type="button"
              className={buttonClass}
              disabled={
                busy ||
                (location.configuredDirectory === location.defaultDirectory &&
                  !location.error)
              }
              onClick={() => void changeFolder(true)}
            >
              Use default folder
            </button>
            <button
              type="button"
              className={buttonClass}
              disabled={busy || !location.activeDirectory || !!location.error}
              onClick={() => {
                void invoke("open_pipeline_dir").catch((reason) =>
                  setError(String(reason)),
                );
              }}
            >
              Open current folder
            </button>
          </div>
          {location.restartRequired && (
            <div
              role="status"
              className="rounded-lg bg-blue-50 px-3 py-2 text-xs leading-5 text-blue-800 dark:bg-blue-950 dark:text-blue-200"
            >
              Saved. Quit and reopen Pipeline to use this folder.
              {location.activeDirectory && (
                <p className="break-all">
                  Until then, data is saved in {location.activeDirectory}.
                </p>
              )}
            </div>
          )}
        </div>
      ) : (
        !error && (
          <p className="mt-4 text-sm" role="status">
            Loading storage settings…
          </p>
        )
      )}
      {(error || location?.error) && (
        <p role="alert" className="mt-3 text-sm text-red-700 dark:text-red-300">
          {error || location?.error}
        </p>
      )}
      <p className="mt-4 text-xs leading-5 text-gray-500 dark:text-neutral-400">
        Changes take effect after restarting Pipeline. Existing data stays in
        its original folder; an empty folder starts a separate research library.
        Select a previous folder to reopen its data.
      </p>
      <details className="mt-3 text-xs leading-5 text-gray-500 dark:text-neutral-400">
        <summary className="cursor-pointer">
          Using Dropbox or another sync service
        </summary>
        <p className="mt-2">
          This setting does not provide cross-device synchronization. The folder
          contains live databases; keep cloud syncing paused while Pipeline is
          running, and do not use the same data on two devices at once.
        </p>
      </details>
    </section>
  );
}
