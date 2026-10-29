import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

export type RootMappings = Record<string, string | null>;

const ABSOLUTE_PATH = /^(\/|[A-Za-z]:[\\/]|\\\\)/;

/**
 * Ask where each project folder recorded in a research backup lives on this
 * computer. An empty field restores that project without a linked folder.
 */
export default function WorkspaceRestoreRootsDialog({
  roots,
  onDone,
}: {
  roots: string[];
  onDone: (mappings: RootMappings | null) => void;
}) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [paths, setPaths] = useState(() => roots.map(() => ""));
  const [error, setError] = useState("");

  useEffect(() => {
    const dialog = dialogRef.current;
    const prior =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    if (dialog && !dialog.open) dialog.showModal();
    return () => {
      if (dialog?.open) dialog.close();
      prior?.focus();
    };
  }, []);

  const setPath = (index: number, value: string) => {
    setError("");
    setPaths((current) =>
      current.map((path, i) => (i === index ? value : path)),
    );
  };

  const choose = async (index: number) => {
    try {
      const picked = await open({ directory: true, multiple: false });
      if (typeof picked === "string") setPath(index, picked);
    } catch (cause) {
      setError(`The folder picker could not open: ${String(cause)}`);
    }
  };

  const submit = () => {
    const trimmed = paths.map((path) => path.trim());
    if (trimmed.some((path) => path && !ABSOLUTE_PATH.test(path))) {
      setError(
        "Enter a full folder path, such as /Users/name/project, or leave the field empty.",
      );
      return;
    }
    onDone(
      Object.fromEntries(roots.map((root, i) => [root, trimmed[i] || null])),
    );
  };

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby="restore-roots-title"
      aria-describedby="restore-roots-description"
      onCancel={(event) => {
        event.preventDefault();
        onDone(null);
      }}
      className="m-auto w-[min(36rem,calc(100%-2rem))] rounded-xl border border-gray-200 bg-white p-0 text-gray-900 shadow-2xl backdrop:bg-black/45 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-100"
    >
      <form
        method="dialog"
        className="space-y-4 p-6"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <h2 id="restore-roots-title" className="text-lg font-semibold">
          Choose project folders
        </h2>
        <p
          id="restore-roots-description"
          className="text-sm leading-6 text-gray-600 dark:text-gray-300"
        >
          This backup links projects to the folders below. Choose where each
          folder is on this computer, or leave a field empty to restore that
          project without a linked folder. You can link a folder later.
        </p>
        {roots.map((root, index) => (
          <div key={root} className="space-y-1">
            <p className="break-all font-mono text-xs text-gray-500">{root}</p>
            <div className="flex gap-2">
              <input
                aria-label={`Folder on this computer for ${root}`}
                value={paths[index]}
                onChange={(event) => setPath(index, event.target.value)}
                placeholder="No linked folder"
                className="min-w-0 flex-1 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-950"
              />
              <button
                type="button"
                aria-label={`Choose folder for ${root}`}
                onClick={() => void choose(index)}
                className="shrink-0 rounded-lg border border-gray-300 px-3 py-2 text-sm dark:border-gray-700"
              >
                Choose…
              </button>
            </div>
          </div>
        ))}
        {error && (
          <p role="alert" className="text-sm text-red-700 dark:text-red-300">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={() => onDone(null)}
            className="rounded-lg border border-gray-300 px-3 py-2 text-sm font-medium dark:border-gray-700"
          >
            Cancel
          </button>
          <button
            type="submit"
            className="rounded-lg bg-gray-900 px-3 py-2 text-sm font-medium text-white hover:bg-gray-800 dark:bg-gray-100 dark:text-gray-900"
          >
            Restore
          </button>
        </div>
      </form>
    </dialog>
  );
}
