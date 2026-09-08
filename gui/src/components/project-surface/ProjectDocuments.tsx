import { addContextObject } from "../WorkspaceContextTray";
import ResearchReaderBoundary from "../ResearchReaderBoundary";
import { lazy, Suspense, useState } from "react";
import { projectClient, type Annotation, type DocumentSelection, type ProjectRecord } from "../../lib/projectClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { button, earlierVersionLabel, earlierVersions, input, linkButton, muted, versionLabel, type SurfaceApi } from "./shared";
const DocumentReader = lazy(() => import("../WorkspaceDocumentReader"));

export type SelectionActionKind = "ask" | "note" | "task" | "revision";

interface Props extends SurfaceApi {
  revisionId: string | null;
  setRevisionId: (id: string | null) => void;
  pinnedRevision: string | null;
  setPinnedRevision: (id: string | null) => void;
  initialSelection: DocumentSelection | undefined;
  setInitialSelection: (selection: DocumentSelection | undefined) => void;
  pinnedSelection: DocumentSelection | undefined;
  selectionAction: { selection: DocumentSelection; action: SelectionActionKind } | null;
  selectionBody: string;
  setSelectionBody: (text: string) => void;
  onSelection: (selection: DocumentSelection, action: SelectionActionKind) => void;
  onSubmitSelection: () => void;
  onCancelSelection: () => void;
  onImport: () => void;
  onAddVersion: () => void;
}

const ACTION_TITLES: Record<SelectionActionKind, string> = { ask: "Question about this passage", note: "Note about this passage", task: "Action item for this passage", revision: "How should this passage change?" };
const ACTION_BUTTONS: Record<SelectionActionKind, string> = { ask: "Open conversation", note: "Save note", task: "Add action item", revision: "Add action item" };

export default function ProjectDocuments(props: Props) {
  const { data, workspaceId, saveSettings, reportError, revisionId, setRevisionId, pinnedRevision, setPinnedRevision, initialSelection, setInitialSelection, pinnedSelection, selectionAction, selectionBody, setSelectionBody, onSelection, onSubmitSelection, onCancelSelection, onImport, onAddVersion, openAnnotation } = props;
  const [mapping, setMapping] = useState("");
  const versions = data.papers.filter(p => p.revision);
  const earlier = [...new Set([...earlierVersions(data), ...(revisionId ? [revisionId] : [])])].filter(id => !data.papers.some(p => p.revision?.id === id));
  const annotations = data.anchors.filter(a => a.body.selection.revisionId === revisionId);
  const layout = data.settings.body.layout === "revision" ? "revision" : "reading";
  const checkMapping = (annotation: ProjectRecord<Annotation>) => {
    if (!revisionId) return;
    void projectClient.map(workspaceId, annotation.id, revisionId).then(result => setMapping(`${result.status}: ${result.explanation}`)).catch(e => reportError(workbenchErrorMessage(e)));
  };
  return <div className="flex min-h-0 flex-1 flex-col p-4">
    <div className="mb-3 flex flex-wrap items-center gap-2">
      <select aria-label="Document" className={`${input} max-w-md`} value={revisionId ?? ""} onChange={e => { setRevisionId(e.target.value || null); setInitialSelection(undefined); }}>
        <option value="">Choose a document</option>
        {versions.map(p => <option key={p.revision!.id} value={p.revision!.id}>{versionLabel(p)}</option>)}
        {earlier.map(id => <option key={id} value={id}>{earlierVersionLabel(id)}</option>)}
      </select>
      <button className={button} onClick={onImport}>Import</button>
      <button className={button} disabled={!revisionId} onClick={()=>{if(revisionId)void projectClient.read(workspaceId,revisionId).then(view=>addContextObject(workspaceId,{kind:"paper",id:view.revision.id,revision:view.revision.contentHash})).catch(e=>reportError(workbenchErrorMessage(e)));}}>Add document to conversation</button>
      <button className={button} disabled={!revisionId} onClick={onAddVersion}>Add new version</button>
      <button className={button} disabled={!revisionId && !pinnedRevision} onClick={() => setPinnedRevision(pinnedRevision ? null : revisionId)}>{pinnedRevision ? "Close comparison" : "Compare with another"}</button>
      <button className={button} onClick={() => void saveSettings({ layout: layout === "reading" ? "revision" : "reading" })}>{layout === "reading" ? "Stack when comparing" : "Side by side when comparing"}</button>
    </div>
    {pinnedRevision && <p className={`mb-2 ${muted}`}>The pinned document stays open while you choose another one above.</p>}
    <div className={`flex min-h-0 flex-1 gap-3 ${layout === "revision" ? "flex-col xl:flex-row" : "flex-col 2xl:flex-row"}`}>
      <Suspense fallback={<p className="p-6 text-sm">Loading reader…</p>}>
        {revisionId
          ? <ResearchReaderBoundary key={revisionId}><DocumentReader key={`${revisionId}:${initialSelection?.start ?? ""}:${initialSelection?.page ?? ""}`} workspaceId={workspaceId} revisionId={revisionId} initialSelection={initialSelection} annotations={annotations} onSelection={onSelection} onError={reportError}/></ResearchReaderBoundary>
          : <p className={`p-8 text-sm ${muted}`}>{versions.length ? "Choose a document above." : "Import a paper to read it here. Select any passage to ask about it, note it, or turn it into an action item."}</p>}
        {pinnedRevision && <ResearchReaderBoundary key={pinnedRevision}><DocumentReader key={`pinned-${pinnedRevision}:${pinnedSelection?.start ?? ""}:${pinnedSelection?.page ?? ""}`} workspaceId={workspaceId} revisionId={pinnedRevision} initialSelection={pinnedSelection} annotations={[]} onSelection={onSelection} onError={reportError} readonly/></ResearchReaderBoundary>}
      </Suspense>
    </div>
    {selectionAction && <form className="mt-3 rounded-lg border border-gray-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-950" onSubmit={e => { e.preventDefault(); onSubmitSelection(); }}>
      <label className="block text-xs">{ACTION_TITLES[selectionAction.action]}
        <textarea aria-label="Selection action text" className={`${input} mt-2`} rows={2} value={selectionBody} onChange={e => setSelectionBody(e.target.value)}/>
      </label>
      <div className="mt-2 flex gap-2">
        <button className={button} disabled={!selectionBody.trim()}>{ACTION_BUTTONS[selectionAction.action]}</button>
        <button className={button} type="button" onClick={onCancelSelection}>Cancel</button>
      </div>
    </form>}
    {data.anchors.length > 0 && <details className={`mt-3 max-h-40 overflow-auto ${muted}`}>
      <summary className="cursor-pointer">Saved passages ({data.anchors.length})</summary>
      {data.anchors.map(a => <div key={a.id} className="mt-2 flex flex-wrap gap-3">
        <button className={`${linkButton} text-left`} onClick={() => openAnnotation(a)}>{a.body.body}</button>
        <button disabled={!revisionId} className={linkButton} onClick={() => checkMapping(a)}>Find in the open document</button>
      </div>)}
      {mapping && <p className="mt-2">{mapping}</p>}
    </details>}
  </div>;
}
