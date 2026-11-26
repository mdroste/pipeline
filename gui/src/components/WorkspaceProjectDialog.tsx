import { useProjectEnvironment } from "../lib/projectEnvironment";
import { useId, useRef, useState } from "react";
import useModalDialog from "../hooks/useModalDialog";
import {
  documentTypes,
  documentRole,
  type DocumentType,
} from "../lib/documentTypes";
import { projectKinds } from "../lib/workspaceNavigation";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { Workspace } from "../lib/workbench/sessionTypes";
import {
  button as secondary,
  input as field,
  primaryButton,
} from "../ui/classes";
import ReportIssueLink from "./ReportIssueLink";
import { findProjectRoots } from "../lib/projectRoots";

const PAPER_EXTENSIONS = ["pdf", "tex", "docx", "md", "txt"];
const basename = (path: string) =>
  path
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .pop() || path;

/**
 * One-step project creation: a name and folder, plus an optional paper. Each
 * step is recorded so a retry after a partial failure does not create a
 * second project or import the paper twice.
 */
export default function WorkspaceProjectDialog({
  onClose,
  onCreated,
  initialFolder,
}: {
  onClose: () => void;
  onCreated: (workspace: Workspace) => Promise<void>;
  initialFolder?: string | null;
}) {
  const {
    workspaceClient,
    researchClient,
    projectClient,
    choose: open,
    embeddedDialogs,
  } = useProjectEnvironment();
  const [name, setName] = useState(
    initialFolder ? basename(initialFolder) : "",
  );
  const [folder, setFolder] = useState<string | null>(initialFolder ?? null);
  const scanned = useRef(false);
  const [roots, setRoots] = useState<string[]>([]);
  const [startFile, setStartFile] = useState("");
  const [scanNotice, setScanNotice] = useState("");
  const [paper, setPaper] = useState<string | null>(null);
  const [documentType, setDocumentType] = useState<DocumentType>("paper");
  const [kind, setKind] = useState("");
  const kindSaved = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const created = useRef<Workspace | null>(null);
  const paperImported = useRef(false);
  const titleId = useId();
  const descriptionId = useId();
  const dialogRef = useModalDialog<HTMLDivElement>(
    () => {
      if (!busy) onClose();
    },
    !embeddedDialogs,
    !embeddedDialogs,
  );

  const chooseFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      setFolder(selected);
      if (!name.trim()) setName(basename(selected));
    }
  };
  const choosePaper = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Document", extensions: PAPER_EXTENSIONS }],
    });
    if (typeof selected === "string") {
      setPaper(selected);
      if (!name.trim()) setName(basename(selected).replace(/\.[^.]+$/, ""));
    }
  };
  const create = async () => {
    const trimmed = name.trim();
    if (!trimmed || !folder || busy) return;
    setBusy(true);
    setError(null);
    try {
      let workspace = created.current;
      if (!workspace) {
        workspace = (
          await workspaceClient.createWorkspace({
            name: trimmed,
            root: folder,
            operationId: `create-workspace-${crypto.randomUUID()}`,
          })
        ).record;
        created.current = workspace;
      }
      if (documentType === "paper" && kind && !kindSaved.current) {
        const home = await projectClient.home(workspace.id);
        await projectClient.mutate(workspace.id, {
          action: "saveHome",
          expectedRevision: home.settings.revision,
          settings: { ...home.settings.body, kind },
        });
        kindSaved.current = true;
      }
      if (paper && !paperImported.current) {
        await researchClient.importPaper({
          workspaceId: workspace.id,
          paperId: null,
          title: basename(paper).replace(/\.[^.]+$/, ""),
          role: documentRole(documentType),
          documentType,
          path: paper,
          operationId: `import-paper-${crypto.randomUUID()}`,
        });
        paperImported.current = true;
      }
      let initial = startFile;
      if (folder && !paper && !scanned.current && !embeddedDialogs) {
        scanned.current = true;
        try {
          const candidates = await findProjectRoots(workspace.id);
          if (candidates.length > 1) {
            setRoots(candidates);
            return;
          }
          initial = candidates[0] ?? "";
        } catch {
          setScanNotice(
            "The folder could not be inspected. You can browse it in Files.",
          );
        }
      }
      if (initial) {
        try {
          sessionStorage.setItem(
            `pipeline.openFile.${workspace.id}`,
            JSON.stringify({ path: initial }),
          );
        } catch {
          /* The folder remains available in Files. */
        }
      }
      await onCreated(workspace);
    } catch (cause) {
      setError(
        `${created.current ? "The project was created, but a later step failed. " : ""}${workbenchErrorMessage(cause)}`,
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      data-project-tour="create"
      role={embeddedDialogs ? "group" : "dialog"}
      aria-modal={embeddedDialogs ? undefined : true}
      aria-labelledby={titleId}
      aria-describedby={descriptionId}
      aria-busy={busy}
      className="fixed inset-0 z-40 flex items-center justify-center bg-[var(--ui-scrim)] p-4"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <form
        className="ui-overlay-frame max-h-full w-full max-w-md space-y-4 overflow-auto p-6"
        onSubmit={(event) => {
          event.preventDefault();
          void create();
        }}
      >
        <div>
          <h2 id={titleId} className="text-base font-semibold">
            New project
          </h2>
          <p
            id={descriptionId}
            className="mt-1 text-xs text-gray-500 dark:text-neutral-400"
          >
            Keep papers, slides, files, and conversations together.
          </p>
        </div>
        <label className="block text-xs">
          Name
          <input
            data-autofocus
            aria-label="Project name"
            className={`${field} mt-1`}
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="e.g. Monetary policy and inequality"
            disabled={busy || Boolean(created.current)}
          />
        </label>
        {documentType === "paper" && (
          <details>
            <summary className="text-xs cursor-pointer">
              Publication details
            </summary>
            <label className="block text-xs">
              Kind of paper <span className="text-gray-500">(optional)</span>
              <select
                aria-label="Kind of paper"
                className={`${field} mt-1`}
                value={kind}
                onChange={(event) => setKind(event.target.value)}
                disabled={busy || kindSaved.current}
              >
                <option value="">Not set</option>
                {projectKinds.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                  </option>
                ))}
              </select>
            </label>
          </details>
        )}
        <div className="text-xs">
          <span className="block">
            Folder <span className="text-gray-500">(required)</span>
          </span>
          <div className="mt-1 flex items-center gap-2">
            <button
              type="button"
              className={secondary}
              disabled={busy || Boolean(created.current?.root)}
              onClick={() =>
                void chooseFolder().catch((cause) =>
                  setError(workbenchErrorMessage(cause)),
                )
              }
            >
              Choose folder…
            </button>
            <span
              className="min-w-0 flex-1 truncate text-gray-600 dark:text-neutral-300"
              title={folder ?? undefined}
            >
              {folder ?? "Research folder or an existing local Git repository."}
            </span>
            {folder && !created.current?.root && (
              <button
                type="button"
                className="text-gray-500 underline"
                disabled={busy}
                onClick={() => setFolder(null)}
              >
                Clear
              </button>
            )}
          </div>
        </div>
        <div className="text-xs">
          <span className="block">
            Document <span className="text-gray-500">(optional)</span>
          </span>
          <div className="mt-1 flex items-center gap-2">
            <button
              type="button"
              className={secondary}
              disabled={busy || paperImported.current}
              onClick={() =>
                void choosePaper().catch((cause) =>
                  setError(workbenchErrorMessage(cause)),
                )
              }
            >
              Choose file…
            </button>
            <span
              className="min-w-0 flex-1 truncate text-gray-600 dark:text-neutral-300"
              title={paper ?? undefined}
            >
              {paper
                ? basename(paper)
                : "A PDF, Word, LaTeX, or text file. You can add more later."}
            </span>
            {paper && !paperImported.current && (
              <button
                type="button"
                className="text-gray-500 underline"
                disabled={busy}
                onClick={() => setPaper(null)}
              >
                Clear
              </button>
            )}
          </div>
        </div>
        {paper && (
          <label className="block text-xs">
            Document type
            <select
              aria-label="Document type"
              className={`${field} mt-1`}
              value={documentType}
              disabled={busy || paperImported.current}
              onChange={(e) => setDocumentType(e.target.value as DocumentType)}
            >
              {documentTypes.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </select>
          </label>
        )}
        {!!roots.length && (
          <label className="block text-xs">
            Several documents found. Start with
            <select
              className={`${field} mt-1`}
              aria-label="Starting document"
              value={startFile}
              onChange={(event) => setStartFile(event.target.value)}
            >
              <option value="">Browse the folder</option>
              {roots.map((path) => (
                <option key={path} value={path}>
                  {path}
                </option>
              ))}
            </select>
            <span className="block mt-1 text-gray-500">
              Opening a file does not build or execute it.
            </span>
          </label>
        )}
        {scanNotice && (
          <p role="status" className="text-xs">
            {scanNotice}
          </p>
        )}
        {error && (
          <p role="alert" className="ui-notice ui-notice--error">
            {error}
            <ReportIssueLink />
          </p>
        )}
        <div className="flex justify-end gap-2">
          {created.current && error && (
            <button
              type="button"
              className={secondary}
              disabled={busy}
              onClick={() => {
                const workspace = created.current;
                if (workspace) void onCreated(workspace);
              }}
            >
              Open project anyway
            </button>
          )}
          <button
            type="button"
            className={secondary}
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            className={primaryButton}
            disabled={!name.trim() || !folder || busy}
          >
            {busy
              ? "Creating…"
              : roots.length && !error
                ? "Open project"
                : created.current
                  ? "Try again"
                  : "Create project"}
          </button>
        </div>
      </form>
    </div>
  );
}
