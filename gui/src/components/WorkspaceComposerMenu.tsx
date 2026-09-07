import WorkspaceIcon from "./WorkspaceIcon";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import type { ConversationSnapshot, Workspace } from "../lib/workbenchTypes";

const WorkspaceAttachmentsPanel = lazy(() => import("./WorkspaceAttachmentsPanel"));
export default function WorkspaceComposerMenu({ snapshot, workspaces, disabled, onProject, onCreateProject, onOpenProject, onSnapshot, onBusy, onResearch, onDictation }: {
  snapshot: ConversationSnapshot | null; workspaces: Workspace[]; disabled: boolean;
  onProject: (id: string | null) => Promise<void>; onCreateProject: () => void; onOpenProject?: () => void;
  onSnapshot: (next: ConversationSnapshot) => void; onBusy: (busy: boolean) => void;
  onResearch: () => void; onDictation: () => void;
}) {
  const [panel, setPanel] = useState<"actions" | "projects" | "files" | "voice" | null>(null);
  const [busy, setBusy] = useState(false);
  const [projectError, setProjectError] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const close = () => { setPanel(null); trigger.current?.focus(); };
  useEffect(() => {
    if (!panel) return;
    const dismiss = (event: PointerEvent) => { if (!busy && !root.current?.contains(event.target as Node)) setPanel(null); };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [panel, busy]);
  useEffect(() => { if (panel) root.current?.querySelector<HTMLElement>("[data-composer-panel] button:not(:disabled)")?.focus(); }, [panel]);
  const buttonClass = "block w-full rounded-lg px-3 py-2.5 text-left text-sm hover:bg-gray-100 disabled:opacity-40 dark:hover:bg-neutral-800";
  return <div ref={root} className="relative" onKeyDown={event => { if (event.key === "Escape" && !busy) { event.stopPropagation(); close(); } }}>
    <button ref={trigger} type="button" aria-label="Add files and tools" aria-expanded={panel !== null} aria-controls="workspace-composer-actions" disabled={disabled && !busy} onClick={() => { if (!busy) setPanel(panel ? null : "actions"); }} className="workspace-add-button"><WorkspaceIcon name="plus" size={20} /></button>
    {panel && <div id="workspace-composer-actions" data-composer-panel className="absolute bottom-full left-0 z-30 mb-3 max-h-[70vh] w-80 max-w-[70vw] overflow-y-auto rounded-2xl border border-gray-200 bg-white p-2 shadow-xl dark:border-neutral-700 dark:bg-neutral-900">
      {panel === "actions" ? <>
        <button type="button" className={buttonClass} onClick={() => setPanel("files")}><span className="mr-3" aria-hidden="true">＋</span>Add files<span className="ml-7 block text-xs text-gray-500">Documents, data, and research code</span></button>
        <button type="button" className={buttonClass} onClick={() => setPanel("projects")}><span className="mr-3" aria-hidden="true">▱</span>Projects<span className="ml-7 block text-xs text-gray-500">{onOpenProject ? "Open this project or switch to another" : "Switch to a project or create one"}</span></button>
        <button type="button" className={buttonClass} onClick={() => setPanel("voice")}><span className="mr-3" aria-hidden="true">≋</span>Voice input<span className="ml-7 block text-xs text-gray-500">System dictation · live voice unavailable</span></button>
        <div className="my-1 border-t dark:border-neutral-800" />
        <button type="button" disabled={!snapshot} className={buttonClass} onClick={() => { close(); onResearch(); }}>Research tools and settings</button>
      </> : <div className="p-2">
        <div className="mb-3 flex items-center justify-between"><button type="button" disabled={busy} onClick={() => setPanel("actions")} className="text-xs text-gray-500">← Back</button><h2 className="text-sm font-semibold">{panel === "files" ? "Add files" : panel === "projects" ? "Select project" : "Voice input"}</h2><button type="button" aria-label="Close composer actions" disabled={busy} onClick={close} className="px-2 text-gray-500">×</button></div>
        {panel === "files" && (snapshot?.session.workspaceId ? <Suspense fallback={<p className="p-3 text-xs text-gray-500">Loading files…</p>}><WorkspaceAttachmentsPanel snapshot={snapshot} onSnapshot={onSnapshot} onBusy={value => { setBusy(value); onBusy(value); }} onClose={close} /></Suspense> : <div className="space-y-3 text-xs"><p className="text-gray-500">Files are saved in projects. Choose or create a project, then open a conversation there to attach files.</p><button type="button" onClick={() => setPanel("projects")} className="rounded-lg border px-3 py-2">Choose a project</button></div>)}
        {panel === "projects" && <>{onOpenProject && snapshot?.workspace && <button type="button" disabled={busy} className={`${buttonClass} mb-2 border dark:border-neutral-800`} onClick={() => { close(); onOpenProject(); }}>Open project: {snapshot.workspace.name}<span className="block text-xs text-gray-500">Paper, folder, notes, tasks, and edits</span></button>}<p className="mb-2 text-xs text-gray-500">Switching shows that project’s conversations. Your current conversation and draft stay where they are.</p><div className="max-h-60 overflow-auto">{[{ id: "", name: "Unfiled conversations" }, ...workspaces.filter(workspace => !workspace.archivedAt)].map(workspace => <button type="button" key={workspace.id} disabled={busy} className={buttonClass} onClick={() => { setBusy(true); setProjectError(null); void onProject(workspace.id || null).then(close).catch(cause => setProjectError(String(cause))).finally(() => setBusy(false)); }}>{workspace.name}{(snapshot?.session.workspaceId ?? "") === workspace.id && <span className="float-right text-gray-400">✓</span>}</button>)}</div>{projectError && <p role="alert" className="text-xs text-red-600">{projectError}</p>}<button type="button" disabled={busy} className={`${buttonClass} border-t dark:border-neutral-800`} onClick={() => { close(); onCreateProject(); }}>+ New project</button></>}
        {panel === "voice" && <div className="space-y-3 text-xs"><p>Live voice conversations are not available in Workspace yet.</p><p className="text-gray-500">To speak a message, focus the message field and use your computer’s system dictation. Review the text before sending.</p><button type="button" disabled={!snapshot} onClick={() => { setPanel(null); onDictation(); }} className="rounded-lg border px-3 py-2">Focus message for dictation</button></div>}
      </div>}
    </div>}
  </div>;
}
