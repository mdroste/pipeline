import { useCallback, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { workbenchClient } from "../lib/workbenchClient";
import { projectClient, type ProjectHome } from "../lib/projectClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  DraftWorkflow,
  ExchangeConflict,
  ExchangeImportPreview,
  ExchangeInspection,
  ExchangePreview,
  ExchangeSelection,
  ExchangeTarget,
  PrunePlan,
  RecipeRun,
  StorageReport,
} from "../lib/workbenchTypes";

const button =
  "rounded border px-3 py-1.5 text-xs hover:bg-gray-50 disabled:opacity-40 dark:border-neutral-700 dark:hover:bg-neutral-800";
const input =
  "rounded border bg-transparent px-2 py-1 text-xs dark:border-neutral-700";
const RECORD_KINDS: Array<[string, string]> = [
  ["task", "Tasks"],
  ["anchor", "Saved selections"],
  ["response", "Findings and referee responses"],
  ["theory", "Theory notes"],
  ["check", "Theory checks"],
  ["direction", "Research directions"],
  ["experiment", "Experiments"],
  ["specification", "Specifications"],
  ["binding", "Result links"],
  ["series", "Result series"],
  ["literature", "Literature notes"],
  ["bibliography", "Bibliography entries"],
  ["build", "Build configurations"],
  ["checkpoint", "Task change sets"],
  ["application", "Acceptance journals"],
];
const DEFAULT_KINDS = ["task", "anchor", "response", "theory", "check", "direction", "experiment", "specification", "binding", "literature", "bibliography"];
const mib = (bytes: number) => `${(bytes / 1048576).toFixed(bytes < 1048576 ? 2 : 1)} MiB`;

interface Props {
  workspaceId: string;
  sessionId: string;
  busy: boolean;
  onAction: (action: () => Promise<void>) => void;
  onError: (message: string) => void;
}

export default function WorkspaceExchangePanel({ workspaceId, sessionId, busy, onAction, onError }: Props) {
  const [kinds, setKinds] = useState<string[]>(DEFAULT_KINDS);
  const [includeNotes, setIncludeNotes] = useState(true);
  const [includeSources, setIncludeSources] = useState(true);
  const [includeLedger, setIncludeLedger] = useState(true);
  const [includePdfs, setIncludePdfs] = useState(false);
  const [allPapers, setAllPapers] = useState(true);
  const [allExecutions, setAllExecutions] = useState(false);
  const [preview, setPreview] = useState<ExchangePreview | null>(null);
  const [inspection, setInspection] = useState<ExchangeInspection | null>(null);
  const [targetKind, setTargetKind] = useState<"newWorkspace" | "existingWorkspace">("newWorkspace");
  const [targetName, setTargetName] = useState("");
  const [importPreview, setImportPreview] = useState<ExchangeImportPreview | null>(null);
  const [conflicts, setConflicts] = useState<ExchangeConflict[]>([]);
  const [storage, setStorage] = useState<StorageReport | null>(null);
  const [pruneKeys, setPruneKeys] = useState<string[]>([]);
  const [prunePlan, setPrunePlan] = useState<PrunePlan | null>(null);
  const [home, setHome] = useState<ProjectHome | null>(null);
  const [runs, setRuns] = useState<RecipeRun[]>([]);
  const [draftName, setDraftName] = useState("Research follow-up");
  const [draftTasks, setDraftTasks] = useState<string[]>([]);
  const [draftRuns, setDraftRuns] = useState<string[]>([]);
  const [draft, setDraft] = useState<DraftWorkflow | null>(null);
  const [notice, setNotice] = useState("");

  const refreshConflicts = useCallback(async () => {
    setConflicts(await workbenchClient.listExchangeConflicts(workspaceId));
  }, [workspaceId]);
  useEffect(() => {
    let disposed = false;
    void Promise.all([
      workbenchClient.listExchangeConflicts(workspaceId),
      projectClient.home(workspaceId),
      workbenchClient.listRecipeRuns(sessionId),
    ])
      .then(([c, h, r]) => {
        if (disposed) return;
        setConflicts(c);
        setHome(h);
        setRuns(r);
      })
      .catch((e) => {
        if (!disposed) onError(workbenchErrorMessage(e));
      });
    return () => {
      disposed = true;
    };
  }, [workspaceId, sessionId, onError]);

  const selection = async (): Promise<ExchangeSelection> => {
    const papers = allPapers ? await workbenchClient.listPapers(workspaceId) : [];
    const executions = allExecutions ? await workbenchClient.listExecutions(workspaceId) : [];
    return {
      includeNotes,
      recordKinds: kinds,
      paperIds: papers.map((p) => p.paper.id),
      executionIds: executions.filter((e) => e.outcome === "completed").map((e) => e.id),
      includeSources,
      includeLedger,
      includeCompiledPdfs: includePdfs,
    };
  };
  const toggle = (list: string[], set: (v: string[]) => void, key: string, on: boolean) =>
    set(on ? [...list, key] : list.filter((k) => k !== key));
  const target = (): ExchangeTarget =>
    targetKind === "newWorkspace"
      ? { kind: "newWorkspace", name: targetName.trim() || `${inspection?.workspaceName ?? "Imported"} (imported)`, root: null }
      : { kind: "existingWorkspace", workspaceId };

  return (
    <div className="space-y-5">
      <section>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Project exchange (coauthor package)</h3>
        <p className="mt-1 text-xs text-gray-500">
          Exports selected research objects with their dependency closure as a readable <code>.pwex</code> package. Conversations, credentials, native runtime state, execution profiles and declared raw inputs are never included.
        </p>
        <div className="mt-2 grid grid-cols-2 gap-1 text-xs md:grid-cols-3">
          {RECORD_KINDS.map(([key, label]) => (
            <label key={key} className="flex gap-2">
              <input type="checkbox" checked={kinds.includes(key)} onChange={(e) => toggle(kinds, setKinds, key, e.target.checked)} />
              {label}
            </label>
          ))}
          <label className="flex gap-2"><input type="checkbox" checked={includeNotes} onChange={(e) => setIncludeNotes(e.target.checked)} />Research notes</label>
          <label className="flex gap-2"><input type="checkbox" checked={includeSources} onChange={(e) => setIncludeSources(e.target.checked)} />Sources</label>
          <label className="flex gap-2"><input type="checkbox" checked={includeLedger} onChange={(e) => setIncludeLedger(e.target.checked)} />Claims and evidence</label>
          <label className="flex gap-2"><input type="checkbox" checked={allPapers} onChange={(e) => setAllPapers(e.target.checked)} />All papers</label>
          <label className="flex gap-2"><input type="checkbox" checked={allExecutions} onChange={(e) => setAllExecutions(e.target.checked)} />All completed executions</label>
          <label className="flex gap-2"><input type="checkbox" checked={includePdfs} onChange={(e) => setIncludePdfs(e.target.checked)} />Compiled PDFs</label>
        </div>
        <div className="mt-2 flex flex-wrap gap-2">
          <button type="button" className={button} disabled={busy} onClick={() => onAction(async () => setPreview(await workbenchClient.previewProjectExchange(workspaceId, await selection())))}>Preview package</button>
          <button type="button" className={button} disabled={busy} onClick={() => onAction(async () => {
            const path = await save({ defaultPath: "project-exchange.pwex", filters: [{ name: "Project exchange package", extensions: ["pwex"] }] });
            if (!path) return;
            const report = await workbenchClient.exportProjectExchange(workspaceId, path, await selection());
            setNotice(`Exported ${report.objectCount} object(s) and ${report.blobCount} blob(s) (${mib(report.bytes)}) to ${report.path}.`);
          })}>Export package…</button>
        </div>
        {preview && (
          <div className="mt-2 rounded border p-2 text-xs dark:border-neutral-800">
            <p>{preview.objects.length} object(s) after dependency closure · {preview.blobCount} blob(s), {mib(preview.blobBytes)}</p>
            <ul className="mt-1 list-inside list-disc text-gray-600 dark:text-gray-400">
              {Object.entries(preview.objects.reduce<Record<string, number>>((acc, o) => ({ ...acc, [o.kind]: (acc[o.kind] ?? 0) + 1 }), {})).map(([k, n]) => <li key={k}>{k}: {n}</li>)}
            </ul>
            {preview.externalReferences.length > 0 && <p className="mt-1 text-amber-700">{preview.externalReferences.length} reference(s) stay external (raw inputs, missing or oversized artifacts).</p>}
            {preview.limitations.map((l) => <p key={l} className="mt-1 text-gray-600 dark:text-gray-400">{l}</p>)}
          </div>
        )}
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <button type="button" className={button} disabled={busy} onClick={() => onAction(async () => {
            const path = await open({ multiple: false, filters: [{ name: "Project exchange package", extensions: ["pwex"] }] });
            if (typeof path !== "string") return;
            const i = await workbenchClient.inspectProjectExchange(path);
            setInspection(i);
            setTargetName(`${i.workspaceName} (imported)`);
            setImportPreview(null);
          })}>Inspect package…</button>
        </div>
        {inspection && (
          <div className="mt-2 space-y-2 rounded border p-2 text-xs dark:border-neutral-800">
            <p>“{inspection.workspaceName}” exported {inspection.createdAt}{inspection.ownExport ? " from this computer" : ""} · {Object.entries(inspection.counts).map(([k, n]) => `${k} ${n}`).join(", ")} · {inspection.blobCount} blob(s), {mib(inspection.blobBytes)}</p>
            {inspection.workspaceRoot && <p className="text-gray-600 dark:text-gray-400">Archived project folder: {inspection.workspaceRoot} (not required; register a folder later if you want file work).</p>}
            {inspection.limitations.map((l) => <p key={l} className="text-gray-600 dark:text-gray-400">{l}</p>)}
            <div className="flex flex-wrap items-center gap-2">
              <select aria-label="Import target" className={input} value={targetKind} onChange={(e) => setTargetKind(e.target.value as typeof targetKind)}>
                <option value="newWorkspace">Into a new separate Workspace (default)</option>
                <option value="existingWorkspace">Merge into this Workspace</option>
              </select>
              {targetKind === "newWorkspace" && <input aria-label="New Workspace name" className={input} value={targetName} onChange={(e) => setTargetName(e.target.value)} />}
              <button type="button" className={button} disabled={busy} onClick={() => onAction(async () => setImportPreview(await workbenchClient.previewProjectExchangeImport(inspection.path, target())))}>Preview import</button>
              <button type="button" className={button} disabled={busy || !importPreview} onClick={() => onAction(async () => {
                const report = await workbenchClient.importProjectExchange(inspection.path, target(), `exchange-${crypto.randomUUID()}`);
                setNotice(`Imported ${report.new + report.remapped} new object(s); ${report.identical} identical; ${report.conflicts} conflict(s) recorded for review; ${report.blobs} blob(s) retained.`);
                setImportPreview(null);
                if (report.workspaceId === workspaceId) await refreshConflicts();
              })}>Import</button>
            </div>
            {importPreview && (
              <p>{importPreview.new} new · {importPreview.remapped} remapped from another namespace · {importPreview.identical} identical (skipped) · {importPreview.conflicts.length} conflict(s) will be recorded and local versions kept.</p>
            )}
          </div>
        )}
        {conflicts.length > 0 && (
          <div className="mt-2 rounded border p-2 text-xs dark:border-neutral-800">
            <p className="font-medium">Import conflicts awaiting review</p>
            {conflicts.map((c) => (
              <div key={c.id} className="mt-1 flex flex-wrap items-center gap-2 border-t pt-1 dark:border-neutral-800">
                <span className="min-w-0 flex-1">{c.objectKind} {c.objectId} · {c.state}</span>
                {c.state === "open" && <>
                  <button type="button" className={button} disabled={busy} onClick={() => onAction(async () => { await workbenchClient.resolveExchangeConflict(workspaceId, c.id, false); await refreshConflicts(); })}>Keep local</button>
                  <button type="button" className={button} disabled={busy} onClick={() => onAction(async () => { await workbenchClient.resolveExchangeConflict(workspaceId, c.id, true); await refreshConflicts(); })}>Take imported</button>
                </>}
                <details className="w-full"><summary className="cursor-pointer">Compare</summary><pre className="max-h-40 overflow-auto whitespace-pre-wrap">{JSON.stringify({ local: c.local, imported: c.imported }, null, 2)}</pre></details>
              </div>
            ))}
          </div>
        )}
      </section>

      <section>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Storage and retention</h3>
        <button type="button" className={`${button} mt-2`} disabled={busy} onClick={() => onAction(async () => { setStorage(await workbenchClient.storageReport()); setPrunePlan(null); })}>Inspect disk use</button>
        {storage && (
          <div className="mt-2 space-y-2 text-xs">
            <p className="text-gray-600 dark:text-gray-400">{storage.note}</p>
            <table className="w-full text-left">
              <thead><tr><th>Category</th><th>Size</th><th>Entries</th><th>Prune</th></tr></thead>
              <tbody>
                {storage.categories.map((c) => (
                  <tr key={c.key} className="border-t align-top dark:border-neutral-800">
                    <td className="py-1 pr-2">{c.label}<p className="text-gray-500">{c.note}</p></td>
                    <td className="py-1 pr-2">{mib(c.bytes)}</td>
                    <td className="py-1 pr-2">{c.entries}</td>
                    <td className="py-1">{c.disposable ? <input type="checkbox" aria-label={`Prune ${c.label}`} checked={pruneKeys.includes(c.key)} onChange={(e) => toggle(pruneKeys, setPruneKeys, c.key, e.target.checked)} /> : <span className="text-gray-500">retained</span>}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="flex flex-wrap gap-2">
              <button type="button" className={button} disabled={busy || !pruneKeys.length} onClick={() => onAction(async () => setPrunePlan(await workbenchClient.pruneStorage(pruneKeys, false)))}>Preview deletion</button>
              <button type="button" className={button} disabled={busy || !prunePlan || prunePlan.applied} onClick={() => onAction(async () => { const plan = await workbenchClient.pruneStorage(pruneKeys, true); setPrunePlan(plan); setStorage(await workbenchClient.storageReport()); setNotice(`Moved ${plan.entries.length} item(s), ${mib(plan.bytes)}, to trash. Restore or empty trash below.`); })}>Move to trash</button>
              <button type="button" className={button} disabled={busy || !storage.trash.length} onClick={() => onAction(async () => { if (!window.confirm(`Permanently delete ${storage.trash.length} trashed item(s)? This cannot be undone.`)) return; await workbenchClient.emptyTrash(); setStorage(await workbenchClient.storageReport()); })}>Empty trash</button>
            </div>
            {prunePlan && !prunePlan.applied && <p>{prunePlan.entries.length} item(s), {mib(prunePlan.bytes)}, would move to trash. Nothing has been moved yet.</p>}
            {storage.trash.map((t) => (
              <div key={t.id} className="flex items-center gap-2 border-t pt-1 dark:border-neutral-800">
                <span className="min-w-0 flex-1 truncate">{t.category} · {mib(t.sizeBytes)} · {t.originalPath}</span>
                <button type="button" className={button} disabled={busy} onClick={() => onAction(async () => { await workbenchClient.restoreTrash(t.id); setStorage(await workbenchClient.storageReport()); })}>Restore</button>
              </div>
            ))}
          </div>
        )}
      </section>

      <section>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Draft a Workflow from research steps</h3>
        <p className="mt-1 text-xs text-gray-500">
          Builds a portable Workflow definition from completed tasks and recipe runs for inspection. Save it and import it in Workflows; validation and the launch preview happen there. Nothing is installed or run from here, and local computations remain listed prerequisites.
        </p>
        <div className="mt-2 grid gap-2 text-xs md:grid-cols-2">
          <div>
            <p className="font-medium">Completed tasks</p>
            {home?.tasks.filter((t) => t.body.status === "completed").map((t) => (
              <label key={t.id} className="flex gap-2"><input type="checkbox" checked={draftTasks.includes(t.id)} onChange={(e) => toggle(draftTasks, setDraftTasks, t.id, e.target.checked)} /><span className="truncate">{t.body.objective}</span></label>
            ))}
            {!home?.tasks.some((t) => t.body.status === "completed") && <p className="text-gray-500">None yet.</p>}
          </div>
          <div>
            <p className="font-medium">Completed recipe runs in this conversation</p>
            {runs.filter((r) => r.status === "completed").map((r) => (
              <label key={r.id} className="flex gap-2"><input type="checkbox" checked={draftRuns.includes(r.id)} onChange={(e) => toggle(draftRuns, setDraftRuns, r.id, e.target.checked)} /><span className="truncate">{r.recipe.name}</span></label>
            ))}
            {!runs.some((r) => r.status === "completed") && <p className="text-gray-500">None yet.</p>}
          </div>
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <input aria-label="Draft workflow name" className={input} value={draftName} onChange={(e) => setDraftName(e.target.value)} />
          <button type="button" className={button} disabled={busy || (!draftTasks.length && !draftRuns.length)} onClick={() => onAction(async () => setDraft(await workbenchClient.draftWorkflow({ workspaceId, name: draftName, recipeRunIds: draftRuns, taskIds: draftTasks, theoryIds: [] })))}>Draft workflow</button>
          <button type="button" className={button} disabled={busy || !draft} onClick={() => onAction(async () => {
            if (!draft) return;
            const path = await save({ defaultPath: `${draft.name.replace(/[^A-Za-z0-9]+/g, "-").toLowerCase() || "workflow"}.json`, filters: [{ name: "Pipeline workflow", extensions: ["json"] }] });
            if (!path) return;
            await invoke("workbench_studio_export_file", { path, content: draft.canonicalJson });
            setNotice(`Saved draft workflow ${draft.fingerprint.slice(0, 19)}… Import it from the Workflows page to validate and preview it.`);
          })}>Save draft…</button>
        </div>
        {draft && (
          <div className="mt-2 rounded border p-2 text-xs dark:border-neutral-800">
            <p>{draft.name} · {draft.steps.length} drafted step(s) plus consolidation · {draft.fingerprint.slice(0, 19)}…</p>
            <ul className="list-inside list-disc">{draft.steps.map((s) => <li key={s.id}>{s.label} <span className="text-gray-500">({s.source})</span></li>)}</ul>
            {draft.unsupported.length > 0 && <div className="mt-1 text-amber-700"><p className="font-medium">Unsupported operations (prerequisites outside the Workflow)</p><ul className="list-inside list-disc">{draft.unsupported.map((u) => <li key={u}>{u}</li>)}</ul></div>}
            {draft.notes.map((n) => <p key={n} className="mt-1 text-gray-600 dark:text-gray-400">{n}</p>)}
          </div>
        )}
      </section>
      <p role="status" className="text-xs">{notice}</p>
    </div>
  );
}
