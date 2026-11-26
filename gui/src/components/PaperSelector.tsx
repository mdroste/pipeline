import { useEffect, useRef, useState } from "react";
import { useReviewSetupEnvironment } from "./reviewSetupEnvironment";
import type {
  InputInterpretation,
  PrimaryInputSelection,
} from "../lib/workflowTypes";
import ReportIssueLink from "./ReportIssueLink";
import { Icon } from "../ui/icons";
import { Menu, MenuItem } from "../ui/Menu";

const DOCUMENT_EXTENSIONS = ["pdf", "tex", "docx", "pptx"];

const isDocumentPath = (path: string) => {
  const name = path.split(/[/\\]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  return (
    dot > 0 && DOCUMENT_EXTENSIONS.includes(name.slice(dot + 1).toLowerCase())
  );
};

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
  const environment = useReviewSetupEnvironment();
  const openDialog = environment.chooseFiles;
  const isFolder = (path: string) =>
    environment.inputFiles(path).then(
      () => true,
      () => false,
    );
  const [ownSelection, setOwnSelection] =
    useState<PrimaryInputSelection | null>(null);
  const controlled = heldSelection !== undefined;
  const selection = controlled ? heldSelection : ownSelection;
  const setSelection = (next: PrimaryInputSelection | null) => {
    if (!controlled) setOwnSelection(next);
  };
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const [inputOptionsOpen, setInputOptionsOpen] = useState(false);
  const optionsRef = useRef<HTMLButtonElement>(null);
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
    void environment
      .onFileDrop((event) => {
        if (disposed || event.type !== "drop" || event.paths.length === 0)
          return;
        const root = rootRef.current;
        const hit = document.elementFromPoint?.(event.x, event.y);
        if (root && hit && root.contains(hit))
          void dropRef.current(event.paths);
      })
      .then((stop) => {
        if (disposed) stop();
        else unsubscribe = stop;
      });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [environment]);

  return (
    <div ref={rootRef} className="review-input-picker">
      <div className="flex gap-2">
        {inputMode !== "folder" && (
          <button
            type="button"
            onClick={handleFile}
            disabled={disabled || picking}
            className="review-file-button flex min-w-0 flex-1 items-center gap-2 rounded-ui-sm border border-line px-2.5 py-2 text-left text-ui-label text-ink transition-colors hover:bg-sunken disabled:opacity-50"
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
          <Icon name="folder-open" className="inline w-4 h-4" />
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
        <div className="review-selection-details mt-2 space-y-1.5">
          <div
            className={`review-selected-path flex min-w-0 items-center gap-3 text-ui-meta ${selection?.selectionKind === "file" && selection.paths.length === 1 ? "review-selected-path--single" : ""}`}
          >
            <p
              className="min-w-0 flex-1 truncate text-ink-muted"
              title={selection?.paths.join(", ") ?? selectedPath}
            >
              {selection?.selectionKind === "file" &&
                selection.paths.length === 1 &&
                selection.interpretation === "batch" &&
                "One-item batch · "}
              {selection?.paths.length === 1
                ? selectedPath
                : `Batch · ${selection?.paths.length} independent reviews`}
            </p>
            <div className="flex gap-2 text-xs">
              <button
                type="button"
                onClick={
                  selection?.selectionKind === "folder" ? handleDir : handleFile
                }
                disabled={disabled || picking}
                className="ui-button ui-button--quiet ui-button--sm"
              >
                Replace
              </button>
              <button
                type="button"
                onClick={clearSelection}
                disabled={disabled}
                className="ui-button ui-button--quiet ui-button--sm"
              >
                Clear selection
              </button>
            </div>
          </div>
          {!(
            selection?.selectionKind === "file" && selection.paths.length > 1
          ) && (
            <div
              className={`review-input-options ${selection?.selectionKind === "file" ? "review-input-options--single" : ""}`}
            >
              <button
                type="button"
                ref={optionsRef}
                disabled={disabled || picking}
                aria-label="Input options"
                aria-haspopup="menu"
                aria-expanded={inputOptionsOpen}
                onClick={() => setInputOptionsOpen(!inputOptionsOpen)}
              >
                {selection?.selectionKind === "folder" ? (
                  selection.interpretation === "latex_project" ? (
                    "LaTeX project"
                  ) : selection.interpretation === "source_tree" ? (
                    "Source folder"
                  ) : (
                    "Batch of documents"
                  )
                ) : (
                  <Icon name="more" className="h-4 w-4" />
                )}
                {selection?.selectionKind === "folder" && (
                  <Icon name="chevron-down" className="h-3 w-3" />
                )}
              </button>
              <Menu
                open={inputOptionsOpen && !disabled && !picking}
                onClose={() => setInputOptionsOpen(false)}
                anchorRef={optionsRef}
                label="Input options"
              >
                {(selection?.selectionKind === "folder"
                  ? [
                      [
                        "latex_project",
                        "One LaTeX document",
                        "Finds the main TeX file and reads its local includes.",
                      ],
                      [
                        "source_tree",
                        "Browsable source folder",
                        "Reviewers can browse and read files in this folder.",
                      ],
                      [
                        "batch",
                        "Batch of documents",
                        "Each supported document directly inside the folder gets its own report.",
                      ],
                    ]
                  : [
                      ["document", "One document"],
                      ["batch", "One-item batch"],
                    ]
                ).map(([mode, label, description]) => (
                  <MenuItem
                    key={mode}
                    description={description}
                    checked={selection?.interpretation === mode}
                    onSelect={() =>
                      setInterpretation(mode as InputInterpretation)
                    }
                  >
                    {label}
                  </MenuItem>
                ))}
              </Menu>
            </div>
          )}
        </div>
      )}
      {pickerError && (
        <p role="alert" className="mt-1.5 text-xs text-danger">
          {pickerError}
          <ReportIssueLink />
        </p>
      )}
    </div>
  );
}
