import { useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { appClient } from "../lib/appClient";
import { onFileDrop } from "../lib/fileDrop";
import type {
  InputInterpretation,
  PrimaryInputSelection,
} from "../lib/workflowTypes";
import ReportIssueLink from "./ReportIssueLink";
import { Icon } from "../ui/icons";

const DOCUMENT_EXTENSIONS = ["pdf", "tex", "docx", "pptx"];

const isDocumentPath = (path: string) => {
  const name = path.split(/[/\\]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  return (
    dot > 0 && DOCUMENT_EXTENSIONS.includes(name.slice(dot + 1).toLowerCase())
  );
};

// Listing a folder's documents fails for anything that is not a folder.
const isFolder = (path: string) =>
  appClient.inputFiles(path).then(
    () => true,
    () => false,
  );

interface Props {
  /** The selection the caller holds. When given, the selector is controlled
   *  and survives remounts; otherwise it keeps its own selection. */
  selection?: PrimaryInputSelection | null;
  onPathChange: (path: string | null) => void;
  onSelectionChange?: (selection: PrimaryInputSelection | null) => void;
  disabled: boolean;
  inputMode?: string;
}

export default function PaperSelector({
  selection: heldSelection,
  onPathChange,
  onSelectionChange,
  disabled,
  inputMode = "document",
}: Props) {
  const [ownSelection, setOwnSelection] =
    useState<PrimaryInputSelection | null>(null);
  const controlled = heldSelection !== undefined;
  const selection = controlled ? heldSelection : ownSelection;
  const setSelection = (next: PrimaryInputSelection | null) => {
    if (!controlled) setOwnSelection(next);
  };
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const applySelection = (next: PrimaryInputSelection) => {
    setSelection(next);
    onPathChange(next.paths[0] ?? null);
    onSelectionChange?.(next);
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
        filters: [
          { name: "Documents and slides", extensions: DOCUMENT_EXTENSIONS },
        ],
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

  // The desktop shell reports dropped files with their real paths; the
  // browser's own drop event does not.
  const receiveDrop = async (paths: string[]) => {
    if (disabled || picking) return;
    setPickerError(null);
    if (inputMode === "folder") {
      const folder = paths.length === 1 ? paths[0] : null;
      if (!folder || !(await isFolder(folder))) {
        setPickerError("Drop one folder, or use Select folder.");
        return;
      }
      applySelection({
        paths: [folder],
        interpretation: "source_tree",
        selectionKind: "folder",
      });
      return;
    }
    const files = paths.filter(isDocumentPath);
    if (files.length === 0) {
      setPickerError(
        "Only PDF, LaTeX, Word, and PowerPoint files can be dropped here. To use a folder, choose it with the folder button.",
      );
      return;
    }
    applySelection({
      paths: files,
      interpretation: files.length > 1 ? "batch" : "document",
      selectionKind: "file",
    });
  };
  const rootRef = useRef<HTMLDivElement>(null);
  const dropRef = useRef(receiveDrop);
  dropRef.current = receiveDrop;
  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | null = null;
    void onFileDrop((event) => {
      if (disposed || event.type !== "drop" || event.paths.length === 0) return;
      const root = rootRef.current;
      const hit = document.elementFromPoint?.(event.x, event.y);
      if (root && hit && root.contains(hit)) void dropRef.current(event.paths);
    }).then((stop) => {
      if (disposed) stop();
      else unsubscribe = stop;
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, []);

  return (
    <div ref={rootRef} className="review-input-picker">
      <div className="flex gap-2">
        {inputMode !== "folder" && (
          <button
            type="button"
            onClick={handleFile}
            disabled={disabled || picking}
            className="review-file-button flex min-w-0 flex-1 items-center gap-3 rounded-ui-md border border-line px-3 py-3 text-left text-ui-body text-ink transition-colors hover:bg-sunken disabled:opacity-50"
          >
            <span className="review-file-icon">
              <Icon name="file" className="h-5 w-5" />
            </span>
            <span className="min-w-0 truncate">
              {selection?.selectionKind === "file"
                ? selection.paths.length === 1
                  ? selectedName
                  : `${selection.paths.length} files selected`
                : "Select files..."}
            </span>
          </button>
        )}
        <button
          type="button"
          onClick={handleDir}
          disabled={disabled || picking}
          className={`ui-button shrink-0 ${inputMode === "folder" ? "flex-1 justify-start" : ""}`}
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
      <p hidden={!!selectedPath} className="mt-2 text-ui-meta text-ink-muted">
        {inputMode === "folder"
          ? "Drop a folder here, or use the folder picker."
          : "Or drop files here · PDF, Word, PowerPoint, or LaTeX"}
      </p>
      {selectedPath && (
        <div className="mt-2 space-y-1.5">
          <div className="flex min-w-0 items-center gap-3 text-ui-meta">
            <p
              className="min-w-0 flex-1 truncate text-ink-muted"
              title={selectedPath}
            >
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
          </div>
          <label className="flex items-center gap-3 text-ui-meta text-ink-muted">
            <span className="shrink-0">Use as</span>
            <select
              aria-label="Input interpretation"
              value={selection?.interpretation}
              onChange={(event) =>
                setInterpretation(event.target.value as InputInterpretation)
              }
              className="min-w-0 flex-1"
            >
              {selection?.selectionKind === "folder" ? (
                <>
                  <option value="latex_project">One LaTeX document</option>
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
          <p
            hidden={selection?.interpretation === "document"}
            className="text-[0.6875rem] leading-4 text-gray-500 dark:text-gray-400"
          >
            {selection?.interpretation === "latex_project"
              ? "Pipeline finds the main TeX file and reads the local files it includes."
              : selection?.interpretation === "source_tree"
                ? "The review can browse and read files in this folder as needed."
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
          <ReportIssueLink />
        </p>
      )}
    </div>
  );
}
