import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

interface Props {
  onPathChange: (path: string | null) => void;
  disabled: boolean;
  inputMode?: string;
}

export default function PaperSelector({ onPathChange, disabled, inputMode = "document" }: Props) {
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);

  const handleFile = async () => {
    setPickerError(null);
    setPicking(true);
    try {
      const path = await openDialog({
        multiple: false,
        filters: [{ name: "Papers", extensions: ["pdf", "tex", "docx"] }],
      });
      if (path) {
        setSelectedPath(path as string);
        onPathChange(path as string);
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setPickerError(`Could not open the file picker: ${message}`);
    } finally {
      setPicking(false);
    }
  };

  const handleDir = async () => {
    setPickerError(null);
    setPicking(true);
    try {
      const path = await openDialog({
        directory: true,
        multiple: false,
      });
      if (path) {
        setSelectedPath(path as string);
        onPathChange(path as string);
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setPickerError(`Could not open the folder picker: ${message}`);
    } finally {
      setPicking(false);
    }
  };

  return (
    <div>
      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">
        Inputs
      </label>
      <div className="flex gap-1.5">
        {inputMode !== "folder" && (
          <button
            type="button"
            onClick={handleFile}
            disabled={disabled || picking}
            className="flex-1 py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                       text-gray-900 dark:text-gray-100
                       hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-50 transition-colors text-left truncate"
          >
            {selectedPath
              ? selectedPath.split(/[/\\]/).pop()
              : "Select file..."}
          </button>
        )}
        <button
          type="button"
          onClick={handleDir}
          disabled={disabled || picking}
          className={`${inputMode === "folder" ? "flex-1 text-left" : ""} py-2 px-2.5 border border-gray-300 dark:border-gray-600 rounded-lg text-gray-500 dark:text-gray-400
                     hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-50 transition-colors shrink-0`}
          title={inputMode === "folder" ? undefined : "Select LaTeX project folder"}
        >
          <svg aria-hidden="true" className="inline w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round"
              d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
          </svg>
          {inputMode === "folder" && (
            <span className="ml-2">
              {selectedPath ? selectedPath.split(/[/\\]/).pop() : "Select folder..."}
            </span>
          )}
        </button>
      </div>
      {selectedPath && (
        <p className="mt-1 text-xs text-gray-500 truncate">{selectedPath}</p>
      )}
      {pickerError && (
        <p role="alert" className="mt-1.5 text-xs text-red-700 dark:text-red-400">
          {pickerError}
        </p>
      )}
    </div>
  );
}
