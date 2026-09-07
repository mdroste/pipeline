import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { Workspace } from "../lib/workbenchTypes";

const PAPER_EXTENSIONS = ["pdf", "tex", "docx", "md", "txt"];
const field = "w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm dark:border-neutral-700 dark:bg-neutral-950";
const secondary = "rounded-md border border-gray-300 px-3 py-1.5 text-xs hover:bg-gray-50 disabled:opacity-40 dark:border-neutral-700 dark:hover:bg-neutral-800";
const basename = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;

/**
 * One-step project creation: a name plus an optional folder and paper. Each
 * step is recorded so a retry after a partial failure does not create a
 * second project or import the paper twice.
 */
export default function WorkspaceProjectDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (workspace: Workspace) => Promise<void> }) {
  const [name, setName] = useState("");
  const [folder, setFolder] = useState<string | null>(null);
  const [paper, setPaper] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const created = useRef<Workspace | null>(null);
  const paperImported = useRef(false);
  const titleId = "workspace-project-dialog-title";
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) onClose(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  const chooseFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") { setFolder(selected); if (!name.trim()) setName(basename(selected)); }
  };
  const choosePaper = async () => {
    const selected = await open({ multiple: false, filters: [{ name: "Paper", extensions: PAPER_EXTENSIONS }] });
    if (typeof selected === "string") { setPaper(selected); if (!name.trim()) setName(basename(selected).replace(/\.[^.]+$/, "")); }
  };
  const create = async () => {
    const trimmed = name.trim();
    if (!trimmed || busy) return;
    setBusy(true); setError(null);
    try {
      let workspace = created.current;
      if (!workspace) {
        workspace = (await workbenchClient.createWorkspace({ name: trimmed, operationId: `create-workspace-${crypto.randomUUID()}` })).record;
        created.current = workspace;
      }
      if (folder && !workspace.root) {
        workspace = (await workbenchClient.registerWorkspaceRoot({ workspaceId: workspace.id, root: folder, expectedRevision: workspace.revision, operationId: `register-root-${crypto.randomUUID()}` })).record;
        created.current = workspace;
      }
      if (paper && !paperImported.current) {
        await workbenchClient.importPaper({ workspaceId: workspace.id, paperId: null, title: basename(paper).replace(/\.[^.]+$/, ""), role: "manuscript", path: paper, operationId: `import-paper-${crypto.randomUUID()}` });
        paperImported.current = true;
      }
      await onCreated(workspace);
    } catch (cause) {
      setError(`${created.current ? "The project was created, but a later step failed. " : ""}${workbenchErrorMessage(cause)}`);
    } finally { setBusy(false); }
  };

  return <div role="dialog" aria-modal="true" aria-labelledby={titleId} className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-4" onPointerDown={event => { if (event.target === event.currentTarget && !busy) onClose(); }}>
    <form className="w-full max-w-md space-y-4 rounded-2xl border border-gray-200 bg-white p-6 shadow-xl dark:border-neutral-700 dark:bg-neutral-900" onSubmit={event => { event.preventDefault(); void create(); }}>
      <div><h2 id={titleId} className="text-base font-semibold">New project</h2><p className="mt-1 text-xs text-gray-500 dark:text-neutral-400">A project keeps a paper, its folder, your notes, and the conversations about it together.</p></div>
      <label className="block text-xs">Name<input autoFocus aria-label="Project name" className={`${field} mt-1`} value={name} onChange={event => setName(event.target.value)} placeholder="e.g. Monetary policy and inequality" disabled={busy || Boolean(created.current)}/></label>
      <div className="text-xs">
        <span className="block">Folder <span className="text-gray-500">(optional)</span></span>
        <div className="mt-1 flex items-center gap-2">
          <button type="button" className={secondary} disabled={busy || Boolean(created.current?.root)} onClick={() => void chooseFolder().catch(cause => setError(workbenchErrorMessage(cause)))}>Choose folder…</button>
          <span className="min-w-0 flex-1 truncate text-gray-600 dark:text-neutral-300" title={folder ?? undefined}>{folder ?? "The folder that holds the paper and its code."}</span>
          {folder && !created.current?.root && <button type="button" className="text-gray-500 underline" disabled={busy} onClick={() => setFolder(null)}>Clear</button>}
        </div>
      </div>
      <div className="text-xs">
        <span className="block">Paper <span className="text-gray-500">(optional)</span></span>
        <div className="mt-1 flex items-center gap-2">
          <button type="button" className={secondary} disabled={busy || paperImported.current} onClick={() => void choosePaper().catch(cause => setError(workbenchErrorMessage(cause)))}>Choose file…</button>
          <span className="min-w-0 flex-1 truncate text-gray-600 dark:text-neutral-300" title={paper ?? undefined}>{paper ? basename(paper) : "A PDF, Word, LaTeX, or text file. You can add more later."}</span>
          {paper && !paperImported.current && <button type="button" className="text-gray-500 underline" disabled={busy} onClick={() => setPaper(null)}>Clear</button>}
        </div>
      </div>
      {error && <p role="alert" className="rounded-md border border-red-300 bg-red-50 p-2 text-xs text-red-800 dark:bg-red-950/30 dark:text-red-200">{error}</p>}
      <div className="flex justify-end gap-2">
        {created.current && error && <button type="button" className={secondary} disabled={busy} onClick={() => { const workspace = created.current; if (workspace) void onCreated(workspace); }}>Open project anyway</button>}
        <button type="button" className={secondary} disabled={busy} onClick={onClose}>Cancel</button>
        <button className="rounded-md bg-gray-900 px-3 py-1.5 text-xs font-medium text-white disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900" disabled={!name.trim() || busy}>{busy ? "Creating…" : created.current ? "Try again" : "Create project"}</button>
      </div>
    </form>
  </div>;
}
