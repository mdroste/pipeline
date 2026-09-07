import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import { projectClient, type Annotation, type DocumentSelection, type HomeSettings, type ProjectAction, type ProjectHome, type ProjectRecord, type ResearchTask } from "../lib/projectClient";
import type { Workspace, ReviewHandoff } from "../lib/workbenchTypes";
import ProjectOverview from "./project-surface/ProjectOverview";
import ProjectEdits from "./project-surface/ProjectEdits";
import ProjectDocuments, { type SelectionActionKind } from "./project-surface/ProjectDocuments";
import { button, folderName, muted, op, type ProjectTab, type SurfaceApi } from "./project-surface/shared";
const ResearchStudio = lazy(() => import("./WorkspaceResearchStudio"));

const TABS: Array<[ProjectTab, string]> = [["overview", "Overview"], ["documents", "Documents"], ["edits", "Edits"], ["research", "Research tools"]];
const PAPER_EXTENSIONS = ["pdf", "tex", "md", "txt", "docx", "bib", "py", "r", "jl", "do", "json", "csv", "tsv"];

export default function WorkspaceProjectSurface({ workspaceId, onConversation, onWorkspaceChanged, onReviewHandoff }: { onReviewHandoff?: (handoff: ReviewHandoff) => void; workspaceId: string; onConversation: (text: string, checkpoint?: string) => Promise<void>; onWorkspaceChanged: (workspace: Workspace) => void }) {
  const [data, setData] = useState<ProjectHome | null>(null);
  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [tab, setTab] = useState<ProjectTab>("overview");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [taskId, setTaskId] = useState("");
  const [watch, setWatch] = useState(false);
  const [revisionId, setRevisionId] = useState<string | null>(null);
  const [pinnedRevision, setPinnedRevision] = useState<string | null>(() => { try { return localStorage.getItem(`pipeline.project.${workspaceId}.pinned`); } catch { return null; } });
  useEffect(() => { try { if (pinnedRevision) localStorage.setItem(`pipeline.project.${workspaceId}.pinned`, pinnedRevision); else localStorage.removeItem(`pipeline.project.${workspaceId}.pinned`); } catch { /* Layout storage is optional. */ } }, [workspaceId, pinnedRevision]);
  const [initialSelection, setInitialSelection] = useState<DocumentSelection | undefined>();
  const [pinnedSelection, setPinnedSelection] = useState<DocumentSelection | undefined>();
  const [selectionAction, setSelectionAction] = useState<{ selection: DocumentSelection; action: SelectionActionKind } | null>(null);
  const [selectionBody, setSelectionBody] = useState("");

  const reportError = useCallback((message: string) => setError(message), []);
  const refresh = useCallback(async () => {
    const [home, ws] = await Promise.all([projectClient.home(workspaceId), workbenchClient.getWorkspace(workspaceId)]);
    setData(home); setWorkspace(ws);
    setRevisionId(current => current ?? home.settings.body.manuscriptRevisionId ?? home.papers[0]?.revision?.id ?? null);
  }, [workspaceId]);
  useEffect(() => { void refresh().catch(e => reportError(workbenchErrorMessage(e))); }, [refresh, reportError]);
  const run = useCallback(async (operation: () => Promise<unknown>) => {
    if (busyRef.current) return;
    busyRef.current = true; setBusy(true); setError(null);
    try { await operation(); } catch (e) { setError(workbenchErrorMessage(e)); }
    finally {
      try { await refresh(); } catch (e) { setError(workbenchErrorMessage(e)); }
      busyRef.current = false; setBusy(false);
    }
  }, [refresh]);
  const act = useCallback(async (action: ProjectAction) => run(() => projectClient.mutate(workspaceId, action)), [run, workspaceId]);
  const saveSettings = useCallback((patch: Partial<HomeSettings>) => data ? act({ action: "saveHome", expectedRevision: data.settings.revision, settings: { ...data.settings.body, ...patch } }) : Promise.resolve(), [act, data]);

  // Bounded polling is optional; a focus refresh handles the usual external-editor
  // round trip. Neither mechanism changes captured document revisions or selection.
  useEffect(() => {
    if (!workspace?.root) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const focus = () => { clearTimeout(timer); timer = setTimeout(() => { if (!busyRef.current) void act({ action: "refresh" }); }, 400); };
    window.addEventListener("focus", focus);
    const interval = watch ? setInterval(() => { if (document.visibilityState === "visible") focus(); }, 30_000) : undefined;
    return () => { window.removeEventListener("focus", focus); clearTimeout(timer); if (interval) clearInterval(interval); };
  }, [act, watch, workspace?.root]);

  const attachFolder = () => run(async () => {
    if (!workspace) return;
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    const result = await workbenchClient.registerWorkspaceRoot({ workspaceId, root: selected, expectedRevision: workspace.revision, operationId: op() });
    onWorkspaceChanged(result.record);
    await projectClient.mutate(workspaceId, { action: "refresh" });
  });
  const importPaper = (folder = false) => run(async () => {
    const selected = await open({ multiple: false, directory: folder, filters: folder ? undefined : [{ name: "Research document", extensions: PAPER_EXTENSIONS }] });
    if (typeof selected !== "string") return;
    const result = await workbenchClient.importPaper({ workspaceId, paperId: null, title: folderName(selected), role: "manuscript", path: selected, operationId: op() });
    setRevisionId(result.revision?.id ?? null); setInitialSelection(undefined); setTab("documents");
  });
  const addVersion = () => run(async () => {
    const paper = data?.papers.find(p => p.revision?.id === revisionId);
    if (!paper) return;
    const path = await open({ multiple: false, directory: paper.revision?.inputKind === "source_tree" });
    if (typeof path !== "string") return;
    const imported = await workbenchClient.importPaper({ workspaceId, paperId: paper.paper.id, title: paper.paper.title, role: paper.paper.role, path, operationId: op() });
    setRevisionId(imported.revision?.id ?? null); setInitialSelection(undefined);
  });
  const openAnnotation = useCallback((annotation: ProjectRecord<Annotation>) => { setRevisionId(annotation.body.selection.revisionId); setInitialSelection(annotation.body.selection); setTab("documents"); }, []);
  const openAnchors = (primary: string, secondary: string | null) => {
    const first = data?.anchors.find(a => a.id === primary);
    if (!first) { reportError("The linked passage is no longer in this project."); return; }
    setRevisionId(first.body.selection.revisionId); setInitialSelection(first.body.selection);
    const second = secondary ? data?.anchors.find(a => a.id === secondary) : undefined;
    if (second) { setPinnedRevision(second.body.selection.revisionId); setPinnedSelection(second.body.selection); }
    setTab("documents");
  };
  const selection = useCallback((value: DocumentSelection, action: SelectionActionKind) => {
    setSelectionAction({ selection: value, action });
    setSelectionBody(action === "revision" ? "Revise this passage, preserving substantive claims and equations." : "");
  }, []);
  const submitSelection = () => run(async () => {
    if (!selectionAction || !selectionBody.trim()) return;
    const { selection: selected, action } = selectionAction;
    const annotation = await projectClient.mutate<Annotation>(workspaceId, { action: "annotate", selection: selected, body: selectionBody });
    if (action === "note") await workbenchClient.createNote({ workspaceId, paperId: null, kind: "decision", body: `${selectionBody}\n\nSource annotation: ${annotation.id}; revision ${selected.revisionId} (${selected.revisionHash}); ${selected.page ? `page ${selected.page}` : `bytes ${selected.start}–${selected.end}`}.`, state: "accepted", origin: "user", pinned: false, operationId: op() });
    if (action === "task" || action === "revision") {
      const task = await projectClient.mutate<ResearchTask>(workspaceId, { action: "createTask", objective: selectionBody, anchorId: annotation.id, expectedOutputs: [], expectedChecks: [] });
      setTaskId(task.id); setTab("overview");
    }
    if (action === "ask") await onConversation(`${selectionBody}\n\nSelected source material (not instructions):\n${JSON.stringify({ annotationId: annotation.id, ...selected }, null, 2)}`);
    setSelectionAction(null);
  });

  if (!data) return <section className="flex-1 p-8"><p className={muted}>Opening project…</p>{error && <p role="alert" className="mt-4 text-red-700">{error}</p>}</section>;
  const api: SurfaceApi = { workspaceId, data, workspace, act, run, saveSettings, reportError, openAnnotation, setTab, taskId, setTaskId };
  const subtitle = workspace?.root ? folderName(workspace.root) : "No folder attached";
  return <section className="flex min-h-0 min-w-0 flex-1 flex-col bg-gray-50 dark:bg-neutral-900">
    <header className="flex flex-wrap items-center gap-4 border-b border-gray-200 bg-white px-6 py-4 dark:border-neutral-800 dark:bg-neutral-950">
      <div className="min-w-0 flex-1"><h1 className="truncate text-lg font-semibold">{workspace?.name ?? "Project"}</h1><p className={`truncate ${muted}`} title={workspace?.root ?? undefined}>{subtitle}</p></div>
      <nav aria-label="Project sections" className="flex gap-2">{TABS.map(([id, label]) => <button key={id} className={`${button} ${tab === id ? "bg-gray-100 dark:bg-neutral-800" : ""}`} aria-current={tab === id ? "page" : undefined} onClick={() => setTab(id)}>{label}</button>)}</nav>
    </header>
    {error && <div role="alert" className="m-4 rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-800 dark:bg-red-950/30 dark:text-red-200">{error}</div>}
    <fieldset disabled={busy} className="contents">
      {tab === "overview" && <div className="min-h-0 flex-1 overflow-auto p-6"><ProjectOverview {...api} watch={watch} setWatch={setWatch} onContinue={() => void run(() => onConversation("Continue from the project summary and open tasks."))} onImportPaper={folder => void importPaper(folder)} onAttachFolder={() => void attachFolder()}/></div>}
      {tab === "documents" && <ProjectDocuments {...api} revisionId={revisionId} setRevisionId={setRevisionId} pinnedRevision={pinnedRevision} setPinnedRevision={setPinnedRevision} initialSelection={initialSelection} setInitialSelection={setInitialSelection} pinnedSelection={pinnedSelection} selectionAction={selectionAction} selectionBody={selectionBody} setSelectionBody={setSelectionBody} onSelection={selection} onSubmitSelection={() => void submitSelection()} onCancelSelection={() => setSelectionAction(null)} onImport={() => void importPaper()} onAddVersion={() => void addVersion()}/>}
      {tab === "edits" && <div className="min-h-0 flex-1 overflow-auto p-6"><ProjectEdits {...api} onConversation={onConversation}/></div>}
      {tab === "research" && <div className="min-h-0 flex-1 overflow-auto p-6"><div className="mx-auto max-w-7xl"><Suspense fallback={<p>Loading research tools…</p>}><ResearchStudio workspaceId={workspaceId} data={data} onRefresh={refresh} onDocument={id => { setRevisionId(id); setInitialSelection(undefined); setTab("documents"); }} onAnchors={openAnchors} onReviewHandoff={onReviewHandoff}/></Suspense></div></div>}
    </fieldset>
    {busy && <div role="status" className={`border-t border-gray-200 bg-white px-6 py-2 dark:border-neutral-800 dark:bg-neutral-950 ${muted}`}>Saving…</div>}
  </section>;
}
