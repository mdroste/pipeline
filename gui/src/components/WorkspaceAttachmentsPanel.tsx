import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { ConversationSnapshot, EffectiveHarness, PaperWithRevision } from "../lib/workbenchTypes";

export default function WorkspaceAttachmentsPanel({ snapshot, onSnapshot, onBusy, onClose }: {
  snapshot: ConversationSnapshot; onSnapshot: (next: ConversationSnapshot) => void; onBusy: (busy: boolean) => void; onClose: () => void;
}) {
  const [papers, setPapers] = useState<PaperWithRevision[]>([]);
  const [harness, setHarness] = useState<EffectiveHarness | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const busyRef = useRef(false);
  const workspaceId = snapshot.session.workspaceId;
  useEffect(() => {
    let disposed = false;
    if (workspaceId) void Promise.all([workbenchClient.listPapers(workspaceId), workbenchClient.effectiveHarness(snapshot.session.id)])
      .then(([nextPapers, nextHarness]) => { if (!disposed) { setPapers(nextPapers); setHarness(nextHarness); } })
      .catch(cause => { if (!disposed) setError(workbenchErrorMessage(cause)); });
    return () => { disposed = true; };
  }, [workspaceId, snapshot.session.id]);

  const act = async (fn: () => Promise<void>) => {
    if (busyRef.current) return;
    busyRef.current = true; setBusy(true); onBusy(true); setError(null);
    try { await fn(); } catch (cause) { setError(workbenchErrorMessage(cause)); }
    finally { busyRef.current = false; setBusy(false); onBusy(false); }
  };
  const importFiles = (directory: boolean) => act(async () => {
    if (!workspaceId) return;
    const paths = await open(directory ? { directory: true, multiple: false } : { multiple: true, filters: [{ name: "Research files", extensions: ["pdf", "docx", "tex", "md", "txt", "bib", "py", "r", "jl", "do", "json", "csv", "tsv"] }] });
    if (!paths) return;
    const failures: string[] = [];
    for (const path of typeof paths === "string" ? [paths] : paths) {
      const name = path.split(/[\\/]/).pop() || "Document";
      try {
        const imported = await workbenchClient.importPaper({ workspaceId, paperId: null, title: name, role: "other", path, operationId: `composer-import-${crypto.randomUUID()}` });
        setPapers(old => [imported, ...old.filter(item => item.paper.id !== imported.paper.id)]);
      } catch (cause) { failures.push(`${name}: ${workbenchErrorMessage(cause)}`); }
    }
    if (failures.length) setError(failures.join("\n"));
  });
  const selectPaper = (paperId: string | null) => act(async () => {
    const latest = await workbenchClient.conversationSnapshot(snapshot.session.id);
    const effective = await workbenchClient.effectiveHarness(snapshot.session.id);
    const enableResearch = paperId !== null && !effective.enabledModules.includes("paper_context");
    const updated = await workbenchClient.updateSession({ sessionId: latest.session.id, expectedRevision: latest.session.revision, operationId: `composer-document-${crypto.randomUUID()}`, paperId: paperId ?? undefined, clearPaper: paperId === null, ...(enableResearch ? { presetId: "research_assistant" } : {}) });
    onSnapshot({ ...latest, session: updated.record, sequence: updated.sequence });
    onClose();
  });
  return <div className="space-y-3">
    <p className="text-xs text-gray-500">Import files into this project, then choose one active document for this conversation. Files remain saved when you change the selection.</p>
    {harness && !harness.enabledModules.includes("paper_context") && <p className="rounded-lg bg-blue-50 p-2 text-xs text-blue-800 dark:bg-blue-950/40 dark:text-blue-200">Selecting a document enables the Research assistant preset so ChatGPT can read it. You can customize this in Research.{snapshot.activeBinding && " This changes the model context for your next message; the saved transcript remains here."}</p>}
    <div className="flex gap-2"><button type="button" disabled={busy} onClick={() => void importFiles(false)} className="rounded-lg bg-gray-900 px-3 py-2 text-xs text-white disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900">{busy ? "Working…" : "Choose files…"}</button><button type="button" disabled={busy} onClick={() => void importFiles(true)} className="rounded-lg border px-3 py-2 text-xs disabled:opacity-40">Add source folder…</button></div>
    {error && <p role="alert" className="whitespace-pre-wrap text-xs text-red-600 dark:text-red-400">{error}</p>}
    <div className="max-h-60 space-y-2 overflow-auto">
      {papers.map(item => <div key={item.paper.id} className="rounded-lg border p-3 dark:border-neutral-700">
        <p className="truncate text-xs font-medium" title={item.paper.title}>{item.paper.title}</p>
        {item.revision?.extraction.status === "failed" && <p className="mt-1 text-xs text-red-600">Text unavailable: {String(item.revision.extraction.error ?? "Extraction failed")}</p>}
        <button type="button" disabled={busy || !harness || item.revision?.extraction.status !== "complete"} onClick={() => void selectPaper(item.paper.id)} className="mt-2 text-xs text-blue-600 underline disabled:text-gray-400 dark:text-blue-400">{snapshot.session.paperId === item.paper.id ? "Selected · use in conversation" : "Use in conversation"}</button>
      </div>)}
      {!papers.length && <p className="py-3 text-xs text-gray-500">No files yet. Add a document, dataset, or research code.</p>}
    </div>
    {snapshot.session.paperId && <button type="button" disabled={busy} onClick={() => void selectPaper(null)} className="text-xs text-gray-500 underline">Use project default document</button>}
  </div>;
}
