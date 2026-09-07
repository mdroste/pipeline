import ResearchTablePreview from "./ResearchTablePreview";
import hljs from "highlight.js/lib/core";
import latex from "highlight.js/lib/languages/latex";
import python from "highlight.js/lib/languages/python";
import r from "highlight.js/lib/languages/r";
import julia from "highlight.js/lib/languages/julia";
import json from "highlight.js/lib/languages/json";
for (const [name, grammar] of Object.entries({ latex, python, r, julia, json })) hljs.registerLanguage(name, grammar);
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ReportViewer from "./ReportViewer";
import { projectClient, textSelection, type Annotation, type DocumentSelection, type DocumentView, type ProjectRecord } from "../lib/projectClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import { workbenchClient } from "../lib/workbenchClient";

const button = "rounded border px-2 py-1 text-xs disabled:opacity-40";
export default function WorkspaceDocumentReader({ workspaceId, revisionId, initialSelection, annotations, onSelection, onError, readonly = false }: {
  workspaceId: string; revisionId: string; initialSelection?: DocumentSelection; annotations: ProjectRecord<Annotation>[];
  onSelection: (selection: DocumentSelection, action: "ask" | "note" | "task" | "revision") => void; onError: (message: string) => void; readonly?: boolean;
}) {
  const key = `pipeline.reader.${workspaceId}.${revisionId}`;
  const saved = useRef<{ page?: number; start?: number; scroll?: number }>({});
  try { saved.current = JSON.parse(localStorage.getItem(key) ?? "{}"); } catch { /* Treat invalid local layout as empty. */ }
  const [start, setStart] = useState(initialSelection?.start ?? saved.current.start ?? 0);
  const [page, setPage] = useState(initialSelection?.page ?? saved.current.page ?? 1);
  const [view, setView] = useState<DocumentView | null>(null);
  const [mode, setMode] = useState<"source" | "rendered" | "page">(initialSelection?.page ? "page" : "source");
  const [selection, setSelection] = useState<DocumentSelection | null>(initialSelection ?? null);
  const [search, setSearch] = useState("");
  const [hits, setHits] = useState<Array<{ start: number; excerpt: string }>>([]);
  const [busy, setBusy] = useState(false);
  const scroll = useRef<HTMLDivElement>(null);
  const source = useRef<HTMLPreElement>(null);
  const drag = useRef<[number, number] | null>(null);
  const mounted = useRef(true);
  const requestSequence = useRef(0);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const load = useCallback(async () => {
    const request = ++requestSequence.current;
    setBusy(true);
    try {
      const next = await projectClient.read(workspaceId, revisionId, start, mode === "page" ? page : null);
      if (!mounted.current || request !== requestSequence.current) return;
      setView(next);
      requestAnimationFrame(() => { if (scroll.current) scroll.current.scrollTop = saved.current.scroll ?? 0; });
    } catch (e) { if (mounted.current && request === requestSequence.current) onError(workbenchErrorMessage(e)); }
    finally { if (mounted.current && request === requestSequence.current) setBusy(false); }
  }, [mode, onError, page, revisionId, start, workspaceId]);
  useEffect(() => { void load(); }, [load]);
  // No polling here: reading remains attached to this immutable revision.
  const selectText = () => {
    if (!view || !source.current) return;
    const selected = window.getSelection();
    if (!selected?.rangeCount || selected.isCollapsed) return;
    const range = selected.getRangeAt(0);
    if (!source.current.contains(range.startContainer) || !source.current.contains(range.endContainer)) return;
    const prefix = range.cloneRange(); prefix.selectNodeContents(source.current); prefix.setEnd(range.startContainer, range.startOffset);
    const offset = prefix.toString().length;
    setSelection(textSelection(view.text, offset, offset + range.toString().length, view));
  };
  const pagePoint = (event: React.PointerEvent<HTMLDivElement>): [number, number] => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return [Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)), Math.max(0, Math.min(1, (event.clientY - bounds.top) / bounds.height))];
  };
  const highlighted = useMemo(() => {
    if (!view || view.text.length > 64_000) return null;
    const extension = view.revision.entrypoint.split(".").pop()?.toLowerCase();
    const language = extension === "tex" ? "latex" : extension === "md" ? "markdown" : extension === "py" ? "python" : extension === "json" ? "json" : null;
    return language ? hljs.highlight(view.text, { language, ignoreIllegals: true }).value : null;
  }, [view]);
  const region = selection?.revisionId === revisionId && selection.page === page ? selection.region : null;
  return <section aria-label="Document reader" className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg border bg-white dark:bg-neutral-950">
    <header className="flex flex-wrap items-center gap-2 border-b p-3">
      <div className="min-w-0 flex-1"><h2 className="truncate text-sm font-semibold">{view?.title ?? "Opening document…"}</h2><p className="text-[11px] text-gray-500">Captured revision {view?.revision.contentHash.slice(0, 12)}{busy && " · Loading…"}</p></div>
      <button className={button} onClick={() => setMode("source")} aria-pressed={mode === "source"}>Text / source</button>
      <button className={button} onClick={() => setMode("rendered")} aria-pressed={mode === "rendered"}>Read with math</button>
      {view?.revision.inputKind === "pdf" && <button className={button} onClick={() => setMode("page")} aria-pressed={mode === "page"}>PDF page</button>}
    </header>
    <form className="flex gap-2 border-b p-2" onSubmit={event => { event.preventDefault(); void workbenchClient.paperSearch(workspaceId, revisionId, search, 20).then(setHits).catch(e => onError(workbenchErrorMessage(e))); }}>
      <input aria-label="Search document" placeholder="Search this revision" value={search} onChange={e => setSearch(e.target.value)} className="min-w-0 flex-1 rounded border bg-transparent px-2 text-sm"/><button disabled={!search.trim()} className={button}>Find</button>
      {mode === "page" ? <><button type="button" className={button} disabled={page <= 1} onClick={() => setPage(v => v - 1)}>Previous</button><label className="text-xs">Page <input aria-label="PDF page" type="number" min={1} value={page} onChange={e => setPage(Math.max(1, Number(e.target.value)))} className="w-14 rounded border bg-transparent px-1"/></label><button type="button" className={button} onClick={() => setPage(v => v + 1)}>Next</button></> : <><button type="button" className={button} disabled={!start} onClick={() => setStart(0)}>Start</button><button type="button" className={button} disabled={!view || view.end >= view.totalBytes} onClick={() => { if (view) { setStart(view.end); setSelection(null); } }}>Next text segment</button></>}
    </form>
    {hits.length > 0 && <div className="max-h-32 overflow-auto border-b p-2">{hits.map(hit => <button key={hit.start} className="block w-full truncate p-1 text-left text-xs hover:bg-gray-100 dark:hover:bg-neutral-800" onClick={() => { setMode("source"); setStart(hit.start); }}>{hit.excerpt}</button>)}</div>}
    {view?.revision.extraction.status !== "complete" && view && <p className="p-2 text-xs text-amber-700">Text extraction is incomplete or unavailable. Inspect the original document or available PDF page view; text search may omit content.</p>}
    {view?.revision.inputKind === "pdf" && <p className="px-3 py-2 text-xs text-gray-500">PDF text can lose equation layout. Inspect page images for visual evidence; drag a region or select the whole page.</p>}
    <div ref={scroll} className="min-h-0 flex-1 overflow-auto p-4" onScroll={event => { try { localStorage.setItem(key, JSON.stringify({ start, page, scroll: event.currentTarget.scrollTop })); } catch { /* Storage is optional. */ } }}>
      {mode === "source" && <pre ref={source} tabIndex={0} onMouseUp={selectText} onKeyUp={selectText} className="whitespace-pre-wrap break-words font-mono text-sm leading-7">{highlighted ? <code dangerouslySetInnerHTML={{ __html: highlighted }}/> : view?.text}</pre>}
      {mode === "rendered" && view && <><p className="mb-3 text-xs text-gray-500">Use Text / source to attach an exact selection.</p>{/\.(csv|tsv)$/i.test(view.revision.entrypoint) ? <ResearchTablePreview text={view.text} path={view.revision.entrypoint}/> : <ReportViewer markdown={view.text}/>}</>}
      {mode === "page" && view?.imageUrl && <div className="relative mx-auto w-fit max-w-full touch-none" onPointerDown={event => { drag.current = pagePoint(event); event.currentTarget.setPointerCapture(event.pointerId); }} onPointerUp={event => { const first = drag.current; drag.current = null; if (!first) return; const last = pagePoint(event); const region: [number, number, number, number] = [Math.min(first[0], last[0]), Math.min(first[1], last[1]), Math.abs(last[0] - first[0]), Math.abs(last[1] - first[1])]; if (region[2] > .005 && region[3] > .005) setSelection({ revisionId, revisionHash: view.revision.contentHash, page, region, start: null, end: null, quote: "" }); }}>
        <img src={view.imageUrl} alt={`${view.title}, page ${page}`} draggable={false} className="max-w-full select-none"/>
        {region && <div className="pointer-events-none absolute border-2 border-blue-600 bg-blue-400/15" style={{ left: `${region[0] * 100}%`, top: `${region[1] * 100}%`, width: `${region[2] * 100}%`, height: `${region[3] * 100}%` }}/>}
      </div>}
      {mode === "page" && view && <button className={`${button} mt-3`} onClick={() => setSelection({ revisionId, revisionHash: view.revision.contentHash, page, region: [0, 0, 1, 1], start: null, end: null, quote: "" })}>Select whole page</button>}
    </div>
    {selection && !readonly && <div className="flex flex-wrap items-center gap-2 border-t bg-blue-50 p-3 dark:bg-blue-950/30"><span className="min-w-0 flex-1 truncate text-xs">{selection.quote || `Page ${selection.page} region`}</span>{([['ask','Ask'],['note','Add note'],['task','Create task'],['revision','Propose revision']] as const).map(([action,label]) => <button key={action} className={button} onClick={() => onSelection(selection, action)}>{label}</button>)}</div>}
    {annotations.length > 0 && <details className="border-t p-3 text-xs"><summary>{annotations.length} saved annotation(s)</summary>{annotations.map(annotation => <button key={annotation.id} className="mt-2 block w-full text-left" onClick={() => { setSelection(annotation.body.selection); if (annotation.body.selection.page) { setMode("page"); setPage(annotation.body.selection.page); } else { setMode("source"); setStart(annotation.body.selection.start ?? 0); } }}>{annotation.body.body}</button>)}</details>}
  </section>;
}
