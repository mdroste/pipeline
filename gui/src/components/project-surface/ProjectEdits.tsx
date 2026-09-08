import SourceDiff from "../file-workspace/SourceDiff";
import HighlightedCode from "../file-workspace/HighlightedCode";
import { fileLanguage } from "../../lib/fileLanguages";
import { studioClient } from "../../lib/studioClient";
import ResearchTablePreview from "../ResearchTablePreview";
import { useEffect, useMemo, useState } from "react";
import { changedPaths, projectClient, type Application, type Checkpoint, type ProjectRecord, type SnapshotPreview } from "../../lib/projectClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { lineDiff } from "../../lib/diff";
import { Card, button, formatDate, input, isOpenTask, linkButton, muted, notice, primaryButton, type SurfaceApi } from "./shared";

interface Props extends SurfaceApi {
  onConversation: (text: string, checkpoint?: string) => Promise<void>;
}

const EDIT_STATES: Record<string, string> = { isolated: "In progress", review: "Ready to review", applied: "Accepted", rejected: "Discarded" };
const editState = (state: string) => EDIT_STATES[state] ?? state.replaceAll("_", " ");

function ChangePreview({ workspaceId, checkpoint, path, onError, onAccepted }: { workspaceId: string; checkpoint: Checkpoint; path: string; onError: (message: string) => void; onAccepted:()=>Promise<void> }) {
  const [sides, setSides] = useState<[SnapshotPreview | null, SnapshotPreview | null] | null>(null);
  useEffect(() => {
    let disposed = false; setSides(null);
    void Promise.all([checkpoint.base[path]?.hash, checkpoint.proposed[path]?.hash].map(hash => hash ? projectClient.preview(workspaceId, hash) : Promise.resolve(null)))
      .then(result => { if (!disposed) setSides(result as [SnapshotPreview | null, SnapshotPreview | null]); })
      .catch(error => { if (!disposed) onError(workbenchErrorMessage(error)); });
    return () => { disposed = true; };
  }, [checkpoint, onError, path, workspaceId]);
  const diff = useMemo(() => sides && sides.every(side => side === null || side.text !== null) ? lineDiff(sides[0]?.text ?? "", sides[1]?.text ?? "") : null, [sides]);
  if (!sides) return <p className={`p-4 ${muted}`}>Loading changes…</p>;
  return <div className="max-h-[32rem] overflow-auto rounded-md border border-gray-200 p-3 dark:border-neutral-800">
    <h4 className="mb-3 font-mono text-xs">{path}</h4>
    {diff
      ? <SourceDiff before={sides[0]?.text ?? ""} after={sides[1]?.text ?? ""} path={path} onAccept={sides[0]?.text != null && sides[1]?.text != null ? async content=>{await studioClient.mutate(workspaceId,{action:"saveText",checkpointId:null,path,expectedHash:sides[0]!.hash,content});await onAccepted();} : undefined}/>
      : <div className="grid grid-cols-2 gap-3">{sides.map((side, i) => <div key={i}><h5 className="mb-2 text-xs">{i ? "Proposed" : "Before"}</h5>{side?.imageUrl ? <img src={side.imageUrl} alt={i ? "Proposed file" : "Original file"}/> : <p className={muted}>{side ? `Binary or large file · ${side.size} bytes · ${side.hash.slice(0, 12)}` : "File absent"}</p>}</div>)}</div>}
  </div>;
}

function AcceptanceHistory({ applications, act }: { applications: ProjectRecord<Application>[]; act: SurfaceApi["act"] }) {
  const label = (state: string) => ({ applied: "Accepted", undone: "Undone", applying: "Interrupted while copying files", recovery_required: "Needs recovery" }[state] ?? state.replaceAll("_", " "));
  if (!applications.length) return null;
  return <section className="space-y-2">
    <h3 className="text-sm font-semibold">Accepted edits</h3>
    {applications.map(a => <div key={a.id} className="rounded-md border border-gray-200 p-3 text-xs dark:border-neutral-800">
      <p>{label(a.body.state)} · {a.body.files.length} file{a.body.files.length === 1 ? "" : "s"}</p>
      {a.body.error && <p className="my-2 text-red-700 dark:text-red-300">{a.body.error}</p>}
      <ul className={`my-2 list-inside list-disc ${muted}`}>{a.body.files.map(f => <li key={f.path}>{f.path}: {f.state}</li>)}</ul>
      {a.body.state === "applied" && <button className={button} onClick={() => void act({ action: "undo", applicationId: a.id })}>Undo</button>}
      {["applying", "recovery_required"].includes(a.body.state) && <button className={button} onClick={() => void act({ action: "recover", applicationId: a.id })}>Restore original files</button>}
    </div>)}
  </section>;
}

function StartEdit({ api }: { api: SurfaceApi }) {
  const { data, workspace, workspaceId, run, taskId, setTaskId, setTab } = api;
  const [fileQuery, setFileQuery] = useState("");
  const [paths, setPaths] = useState<string[]>([]);
  const [backend, setBackend] = useState("copy");
  const [filePreview, setFilePreview] = useState<{ path: string; value: SnapshotPreview } | null>(null);
  const files = data.inventory?.body.files.filter(f => f.hash && f.path.toLowerCase().includes(fileQuery.toLowerCase())) ?? [];
  const openTasks = data.tasks.filter(t => isOpenTask(t.body.status));
  const ready = data.fileAcceptance && Boolean(workspace?.root) && Boolean(taskId) && paths.length > 0;
  return <Card title="Start an edit">
    <p className={muted}>Choose the files the assistant may change. They are copied into a separate working copy, so nothing in your folder changes until you accept the edits below.</p>
    {!workspace?.root && <p className={notice}>Attach a folder on the Overview tab first. <button className={linkButton} onClick={() => setTab("overview")}>Go to Overview</button></p>}
    {workspace?.root && !data.fileAcceptance && <p className={notice}>On this platform edits can be made in a working copy but cannot be copied back into your folder.</p>}
    {workspace?.root && <>
      <label className="block text-xs">Action item this edit is for
        <select aria-label="Action item for this edit" className={`${input} mt-1`} value={taskId} onChange={e => setTaskId(e.target.value)}>
          <option value="">{openTasks.length ? "Choose an action item" : "No open action items; add one on the Action items tab"}</option>
          {openTasks.map(t => <option key={t.id} value={t.id}>{t.body.objective}</option>)}
        </select>
      </label>
      <div className="flex flex-wrap gap-2">
        <input aria-label="Filter files" placeholder="Filter files" className={`${input} max-w-sm`} value={fileQuery} onChange={e => setFileQuery(e.target.value)}/>
        <button className={button} onClick={() => setPaths(files.map(f => f.path))}>Select shown files</button>
        <button className={button} onClick={() => setPaths([])}>Clear</button>
      </div>
      <div className="max-h-56 overflow-auto rounded-md border border-gray-200 dark:border-neutral-800">
        {!files.length && <p className={`p-3 ${muted}`}>{data.inventory ? "No files match." : "Refresh the file list on the Overview tab to see the folder’s files."}</p>}
        {files.slice(0, 200).map(f => <label key={f.path} className="flex gap-2 px-3 py-1.5 text-xs">
          <input type="checkbox" checked={paths.includes(f.path)} onChange={e => setPaths(old => e.target.checked ? [...old, f.path] : old.filter(p => p !== f.path))}/>
          <span className="min-w-0 flex-1 break-all font-mono">{f.path}</span>
          <span className={muted}>{Math.ceil(f.size / 1024)} KiB</span>
          <button type="button" aria-label={`Preview ${f.path}`} className={linkButton} onClick={e => { e.preventDefault(); void run(async () => setFilePreview({ path: f.path, value: await projectClient.workingFile(workspaceId, f.path) })); }}>Preview</button>
        </label>)}
        {files.length > 200 && <p className={`p-3 ${muted}`}>Showing 200 of {files.length} files; narrow the filter to see more. “Select shown files” selects all {files.length}.</p>}
      </div>
      {filePreview && <div className="max-h-96 space-y-3 overflow-auto rounded-md border border-gray-200 p-4 dark:border-neutral-800">
        <div className="flex justify-between gap-3 text-xs"><p className="font-mono">{filePreview.path}</p><button className={linkButton} onClick={() => setFilePreview(null)}>Close</button></div>
        {filePreview.value.imageUrl ? <img alt={filePreview.path} src={filePreview.value.imageUrl} className="max-h-80"/> : filePreview.value.text !== null ? /\.(csv|tsv)$/i.test(filePreview.path) ? <ResearchTablePreview text={filePreview.value.text} path={filePreview.path}/> : <HighlightedCode text={filePreview.value.text} language={fileLanguage(filePreview.path)}/> : <p className="text-xs">Binary or large file · {filePreview.value.size} bytes. Import PDFs as papers to read them page by page.</p>}
      </div>}
      <div className="flex flex-wrap items-center gap-3">
        <label className="text-xs">Working copy
          <select aria-label="Working copy method" className="ml-2 rounded-md border border-gray-300 bg-transparent p-1.5 text-xs dark:border-neutral-700" value={backend} onChange={e => setBackend(e.target.value)}>
            <option value="copy">Plain copy of the files</option>
            <option value="git">Git branch in a separate worktree</option>
          </select>
        </label>
        <button className={primaryButton} disabled={!ready} onClick={() => void run(async () => { await projectClient.mutate<Checkpoint>(workspaceId, { action: "checkpoint", taskId, paths, backend }); setPaths([]); })}>Copy {paths.length} file{paths.length === 1 ? "" : "s"} and start</button>
      </div>
      {backend === "git" && <p className={muted}>Creates a new branch under codex/ and a worktree for it, then copies your selected files in. Your current branch, index, and uncommitted work are not touched. Nothing is committed or pushed.</p>}
    </>}
  </Card>;
}

export default function ProjectEdits(props: Props) {
  const { data, workspaceId, act, reportError, onConversation } = props;
  const [selectedId, setSelectedId] = useState<string>(data.changes[0]?.id ?? "");
  const selected = data.changes.find(c => c.id === selectedId) ?? data.changes[0];
  const paths = useMemo(() => selected ? changedPaths(selected.body) : [], [selected]);
  const [accepted, setAccepted] = useState<string[]>([]);
  const [preview, setPreview] = useState("");
  useEffect(() => { setAccepted([]); setPreview(""); }, [selected?.id, selected?.revision]);
  const active = selected ? ["isolated", "review"].includes(selected.body.state) : false;
  const taskFor = (checkpoint: ProjectRecord<Checkpoint>) => data.tasks.find(t => t.id === checkpoint.body.taskId)?.body.objective ?? checkpoint.id;

  return <div className="mx-auto max-w-6xl space-y-6">
    <StartEdit api={props}/>
    <Card title="Edits">
      {!selected && <p className={muted}>No edits yet. Start one above; the assistant’s proposed changes will appear here for review.</p>}
      {selected && <>
        <select aria-label="Edit" className={input} value={selected.id} onChange={e => setSelectedId(e.target.value)}>{data.changes.map(c => <option key={c.id} value={c.id}>{taskFor(c).slice(0, 80)} · {editState(c.body.state)} · {formatDate(c.body.capturedAt)}</option>)}</select>
        <div className="rounded-lg border border-gray-200 p-4 dark:border-neutral-800">
          <p className="text-sm">{selected.body.instruction.split("\n")[0]}</p>
          <p className={`mt-2 break-all font-mono ${muted}`}>{selected.body.taskRoot}</p>
          <p className={`mt-2 ${muted}`}>{selected.body.backend === "git" ? "Working copy on a separate Git branch; your own branch and index are untouched." : "Working copy made from plain copies of the selected files."}</p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button className={primaryButton} disabled={!active} onClick={() => void onConversation("", selected.id).catch(e => reportError(workbenchErrorMessage(e)))}>Open conversation</button>
            <button className={button} disabled={!active} onClick={() => void act({ action: "captureChanges", checkpointId: selected.id })}>Check for changes</button>
            <button className={button} disabled={!active} onClick={() => void act({ action: "reject", checkpointId: selected.id })}>Discard this edit</button>
          </div>
        </div>
        {selected.body.state !== "isolated" && <>
          <div className="grid gap-4 xl:grid-cols-[18rem_1fr]">
            <div className="space-y-2">
              <p className={muted}>Tick the files to copy back into your folder. Files you did not select are left alone.</p>
              <button className={button} disabled={!paths.length} onClick={() => setAccepted(paths)}>Select all</button>
              {paths.map(path => <div key={path} className="flex items-center gap-2 rounded-md border border-gray-200 p-2 text-xs dark:border-neutral-800">
                <input type="checkbox" aria-label={`Accept ${path}`} checked={accepted.includes(path)} onChange={e => setAccepted(old => e.target.checked ? [...old, path] : old.filter(p => p !== path))}/>
                <button className="min-w-0 flex-1 break-all text-left font-mono" onClick={() => setPreview(path)}>{path}</button>
              </div>)}
              {!paths.length && <p className={muted}>No files changed.</p>}
            </div>
            {preview ? <ChangePreview workspaceId={workspaceId} checkpoint={selected.body} path={preview} onError={reportError} onAccepted={()=>props.act({action:"refresh"})}/> : <p className={`rounded-md border border-gray-200 p-5 dark:border-neutral-800 ${muted}`}>Select a file to see what changed.</p>}
          </div>
          <div className="rounded-lg border border-gray-200 p-4 dark:border-neutral-800">
            <button className={primaryButton} disabled={!data.fileAcceptance || selected.body.state !== "review" || !accepted.length} onClick={() => void act({ action: "apply", checkpointId: selected.id, paths: accepted, expectedRevision: selected.revision })}>Accept {accepted.length} file{accepted.length === 1 ? "" : "s"}</button>
            <p className={`mt-3 ${muted}`}>{selected.body.checkedExecutionIds.length ? `${selected.body.checkedExecutionIds.length} completed run${selected.body.checkedExecutionIds.length === 1 ? "" : "s"} recorded for this edit.` : "No runs were recorded for this edit."} Accepting copies files; it does not check that the results are correct. Rebuild or rerun after accepting.</p>
            {selected.body.limitations.map((limit, i) => <p key={i} className={`mt-2 ${muted}`}>{limit}</p>)}
          </div>
        </>}
      </>}
    </Card>
    <AcceptanceHistory applications={data.applications} act={act}/>
  </div>;
}
