import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { InputInterpretation, PrimaryInputSelection } from "../lib/types";

const RECENT_INPUTS_KEY = "pipeline.recentInputs.v1";

function loadRecentInputs(): PrimaryInputSelection[] {
  try {
    const parsed = JSON.parse(
      localStorage.getItem(RECENT_INPUTS_KEY) ?? "[]",
    ) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((item): item is PrimaryInputSelection => {
        if (!item || typeof item !== "object") return false;
        const candidate = item as Partial<PrimaryInputSelection>;
        return (
          Array.isArray(candidate.paths) &&
          candidate.paths.every((path) => typeof path === "string") &&
          typeof candidate.interpretation === "string" &&
          (candidate.selectionKind === "file" ||
            candidate.selectionKind === "folder")
        );
      })
      .slice(0, 5);
  } catch {
    return [];
  }
}

interface Props {
  onPathChange: (path: string | null) => void;
  onSelectionChange?: (selection: PrimaryInputSelection | null) => void;
  disabled: boolean;
  inputMode?: string;
}

export default function PaperSelector({
  onPathChange,
  onSelectionChange,
  disabled,
  inputMode = "document",
}: Props) {
  const [selection, setSelection] = useState<PrimaryInputSelection | null>(
    null,
  );
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const [recentInputs, setRecentInputs] =
    useState<PrimaryInputSelection[]>(loadRecentInputs);

  const applySelection = (next: PrimaryInputSelection) => {
    setSelection(next);
    onPathChange(next.paths[0] ?? null);
    onSelectionChange?.(next);
    const key = `${next.selectionKind}:${next.interpretation}:${next.paths.join("\0")}`;
    const updated = [
      next,
      ...recentInputs.filter(
        (item) =>
          `${item.selectionKind}:${item.interpretation}:${item.paths.join("\0")}` !==
          key,
      ),
    ].slice(0, 5);
    setRecentInputs(updated);
    try {
      localStorage.setItem(RECENT_INPUTS_KEY, JSON.stringify(updated));
    } catch {
      // Recent choices are a convenience; selection itself remains valid.
    }
  };

  const clearSelection = () => {
    setSelection(null);
    setPickerError(null);
    onPathChange(null);
    onSelectionChange?.(null);
  };

  const handleFile = async () => {
    setPickerError(null);
    setPicking(true);
    try {
      const path = await openDialog({
        multiple: true,
        filters: [{ name: "Papers", extensions: ["pdf", "tex", "docx"] }],
      });
      if (path) {
        const paths = (Array.isArray(path) ? path : [path]) as string[];
        const next: PrimaryInputSelection = {
          paths,
          interpretation: paths.length > 1 ? "batch" : "document",
          selectionKind: "file",
        };
        applySelection(next);
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
        const next: PrimaryInputSelection = {
          paths: [path as string],
          interpretation:
            inputMode === "folder" ? "source_tree" : "latex_project",
          selectionKind: "folder",
        };
        applySelection(next);
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setPickerError(`Could not open the folder picker: ${message}`);
    } finally {
      setPicking(false);
    }
  };

  const setInterpretation = (interpretation: InputInterpretation) => {
    if (!selection) return;
    const next = { ...selection, interpretation };
    setSelection(next);
    onSelectionChange?.(next);
  };

  const selectedPath = selection?.paths[0] ?? null;
  const selectedName = selectedPath?.split(/[/\\]/).pop();

  const handleDrop = (event: React.DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    if (disabled) return;
    const paths = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path));
    if (paths.length === 0) {
      setPickerError(
        "This drag did not provide local file paths. Use Select files instead.",
      );
      return;
    }
    setPickerError(null);
    applySelection({
      paths,
      interpretation: paths.length > 1 ? "batch" : "document",
      selectionKind: "file",
    });
  };

  return (
    <div onDragOver={(event) => event.preventDefault()} onDrop={handleDrop}>
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
            {selection?.selectionKind === "file"
              ? selection.paths.length === 1
                ? selectedName
                : `${selection.paths.length} files selected`
              : "Select files..."}
          </button>
        )}
        <button
          type="button"
          onClick={handleDir}
          disabled={disabled || picking}
          className={`${inputMode === "folder" ? "flex-1 text-left" : ""} py-2 px-2.5 border border-gray-300 dark:border-gray-600 rounded-lg text-gray-500 dark:text-gray-400
                     hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-50 transition-colors shrink-0`}
          title="Select folder"
        >
          <svg
            aria-hidden="true"
            className="inline w-4 h-4"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={1.5}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z"
            />
          </svg>
          {inputMode === "folder" && (
            <span className="ml-2">
              {selectedPath
                ? selectedPath.split(/[/\\]/).pop()
                : "Select folder..."}
            </span>
          )}
        </button>
      </div>
      <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">
        Drop local files here, or use the file and folder pickers.
      </p>
      {!selection && recentInputs.length > 0 && (
        <label className="mt-2 block text-xs text-gray-600 dark:text-gray-300">
          Recent input
          <select
            aria-label="Choose a recent input"
            defaultValue=""
            disabled={disabled}
            onChange={(event) => {
              const index = Number(event.target.value);
              if (Number.isInteger(index) && recentInputs[index])
                applySelection(recentInputs[index]);
              event.target.value = "";
            }}
            className="mt-1 w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-600 dark:bg-gray-800"
          >
            <option value="">Choose recent…</option>
            {recentInputs.map((item, index) => (
              <option key={`${item.paths.join("|")}-${index}`} value={index}>
                {item.paths.length === 1
                  ? item.paths[0]
                  : `${item.paths.length} files`}
              </option>
            ))}
          </select>
        </label>
      )}
      {selectedPath && (
        <div className="mt-2 space-y-1.5">
          <p className="text-xs text-gray-500 truncate" title={selectedPath}>
            {selection?.paths.length === 1
              ? selectedPath
              : selection?.paths.join(", ")}
          </p>
          <div className="flex gap-2 text-xs">
            <button
              type="button"
              onClick={
                selection?.selectionKind === "folder" ? handleDir : handleFile
              }
              disabled={disabled || picking}
              className="text-gray-600 underline underline-offset-2 dark:text-gray-300"
            >
              Replace
            </button>
            <button
              type="button"
              onClick={clearSelection}
              disabled={disabled}
              className="text-red-600 underline underline-offset-2 dark:text-red-400"
            >
              Clear
            </button>
          </div>
          <label className="block text-xs font-medium text-gray-600 dark:text-gray-300">
            Use this{" "}
            {selection?.selectionKind === "folder" ? "folder" : "selection"} as
            <select
              aria-label="Input interpretation"
              value={selection?.interpretation}
              onChange={(event) =>
                setInterpretation(event.target.value as InputInterpretation)
              }
              className="mt-1 w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                         text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                         focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent
                         transition-colors disabled:opacity-50"
            >
              {selection?.selectionKind === "folder" ? (
                <>
                  <option value="latex_project">One LaTeX paper</option>
                  <option value="source_tree">Browsable source folder</option>
                  <option value="batch">Batch of documents</option>
                </>
              ) : selection && selection.paths.length > 1 ? (
                <option value="batch">Independent batch jobs</option>
              ) : (
                <>
                  <option value="document">One document</option>
                  <option value="batch">One-item batch</option>
                </>
              )}
            </select>
          </label>
          <p className="text-[11px] leading-4 text-gray-500 dark:text-gray-400">
            {selection?.interpretation === "latex_project"
              ? "Pipeline finds the project’s top-level main TeX file and resolves its local includes."
              : selection?.interpretation === "source_tree"
                ? "The workflow receives an inventory and may read files inside this folder on demand."
                : selection?.interpretation === "batch"
                  ? selection.selectionKind === "folder"
                    ? "Each supported document directly inside this folder becomes an independent report."
                    : "Each selected document becomes an independent report."
                  : "Pipeline extracts this as one document."}
          </p>
        </div>
      )}
      {pickerError && (
        <p
          role="alert"
          className="mt-1.5 text-xs text-red-700 dark:text-red-400"
        >
          {pickerError}
        </p>
      )}
    </div>
  );
}
