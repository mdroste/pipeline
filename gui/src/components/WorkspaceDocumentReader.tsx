import ResearchTablePreview from "./ResearchTablePreview";
import { fileLanguage, highlightedSource } from "../lib/fileLanguages";
import { FileNavigationContext } from "./file-workspace/FileNavigation";
import { workspaceFileAdapter } from "../lib/fileWorkspaceClient";
import type { FileLocation } from "../lib/fileLinks";
const PdfReader = lazy(() => import("./file-workspace/PdfReader"));
const FileWorkspace = lazy(() => import("./file-workspace/FileWorkspace"));
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import ReportViewer from "./ReportViewer";
import {
  projectClient,
  textSelection,
  type Annotation,
  type DocumentSelection,
  type DocumentView,
  type ProjectRecord,
} from "../lib/projectClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import { researchClient } from "../lib/workbench/researchClient";
import useDraftText from "../hooks/useDraftText";

const button = "rounded border px-2 py-1 text-xs disabled:opacity-40";
/** A whole page number of at least 1; anything else reads as page 1. */
const pageNumber = (value: unknown) =>
  Math.max(1, Math.floor(Number(value)) || 1);
export default function WorkspaceDocumentReader({
  workspaceId,
  revisionId,
  initialSelection,
  annotations,
  onSelection,
  onError,
  readonly = false,
}: {
  workspaceId: string;
  revisionId: string;
  initialSelection?: DocumentSelection;
  annotations: ProjectRecord<Annotation>[];
  onSelection: (
    selection: DocumentSelection,
    action: "ask" | "note" | "task" | "revision",
  ) => void;
  onError: (message: string) => void;
  readonly?: boolean;
}) {
  const adapter = useMemo(
    () => workspaceFileAdapter({ workspaceId, revisionId }),
    [workspaceId, revisionId],
  );
  const [linkedFile, setLinkedFile] = useState<FileLocation | null>(null);
  const [interactivePdf, setInteractivePdf] = useState(true);
  const key = `pipeline.reader.${workspaceId}.${revisionId}`;
  const saved = useRef<{
    page?: number;
    start?: number;
    scroll?: number;
    mode?: "source" | "rendered" | "page";
  }>({});
  try {
    saved.current = JSON.parse(localStorage.getItem(key) ?? "{}");
  } catch {
    /* Treat invalid local layout as empty. */
  }
  const [start, setStart] = useState(
    initialSelection?.start ?? saved.current.start ?? 0,
  );
  const [page, setPage] = useState(() =>
    pageNumber(initialSelection?.page ?? saved.current.page),
  );
  const [view, setView] = useState<DocumentView | null>(null);
  const [mode, setMode] = useState<"source" | "rendered" | "page">(
    initialSelection?.page ? "page" : (saved.current.mode ?? "source"),
  );
  // The saved scroll offset belongs to the text segment it was measured in.
  // An opened passage begins at the top of its segment, so it starts there.
  const layout = useRef({
    start,
    page,
    mode,
    scroll:
      !initialSelection && start === saved.current.start
        ? (saved.current.scroll ?? 0)
        : 0,
  });
  const writeLayout = () => {
    try {
      localStorage.setItem(key, JSON.stringify(layout.current));
    } catch {
      /* Optional reader layout. */
    }
  };
  useEffect(() => {
    const scroll = start === layout.current.start ? layout.current.scroll : 0;
    layout.current = { start, page, mode, scroll };
    writeLayout();
  }, [key, start, page, mode]);
  const scrollWrite = useRef(0);
  useEffect(() => () => cancelAnimationFrame(scrollWrite.current), []);
  const pdfPosition = useMemo(() => ({ page }), [page]);
  const pageCount = view?.pageCount ?? null;
  const goToPage = (value: unknown) => {
    const next = pageNumber(value);
    setPage(pageCount ? Math.min(pageCount, next) : next);
  };
  const pageInput = useDraftText(String(page), goToPage);
  const [selection, setSelection] = useState<DocumentSelection | null>(
    initialSelection ?? null,
  );
  const [search, setSearch] = useState("");
  const [hits, setHits] = useState<Array<{ start: number; excerpt: string }>>(
    [],
  );
  const [busy, setBusy] = useState(false);
  const scroll = useRef<HTMLDivElement>(null);
  const source = useRef<HTMLPreElement>(null);
  const drag = useRef<[number, number] | null>(null);
  const mounted = useRef(true);
  const requestSequence = useRef(0);
  const shown = useRef<{ start: number; page: number | null } | null>(null);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const renderPage = mode === "page" && !interactivePdf ? page : null;
  const load = useCallback(async () => {
    const request = ++requestSequence.current;
    setBusy(true);
    try {
      const next = await projectClient.read(
        workspaceId,
        revisionId,
        start,
        renderPage,
      );
      if (!mounted.current || request !== requestSequence.current) return;
      setView(next);
      // Restore the saved offset on the first load only. A new segment, search
      // hit or page starts at its top, where the requested passage begins.
      const previous = shown.current;
      shown.current = { start: next.start, page: next.page };
      const top = !previous
        ? layout.current.scroll
        : previous.start !== next.start || previous.page !== next.page
          ? 0
          : null;
      if (top !== null)
        requestAnimationFrame(() => {
          if (scroll.current) scroll.current.scrollTop = top;
        });
    } catch (e) {
      if (mounted.current && request === requestSequence.current)
        onError(workbenchErrorMessage(e));
    } finally {
      if (mounted.current && request === requestSequence.current)
        setBusy(false);
    }
  }, [renderPage, onError, revisionId, start, workspaceId]);
  useEffect(() => {
    void load();
  }, [load]);
  // No polling here: reading remains attached to this immutable revision.
  const selectText = () => {
    if (!view || !source.current) return;
    const selected = window.getSelection();
    if (!selected?.rangeCount || selected.isCollapsed) return;
    const range = selected.getRangeAt(0);
    if (
      !source.current.contains(range.startContainer) ||
      !source.current.contains(range.endContainer)
    )
      return;
    const prefix = range.cloneRange();
    prefix.selectNodeContents(source.current);
    prefix.setEnd(range.startContainer, range.startOffset);
    const offset = prefix.toString().length;
    setSelection(
      textSelection(view.text, offset, offset + range.toString().length, view),
    );
  };
  const pagePoint = (
    event: React.PointerEvent<HTMLDivElement>,
  ): [number, number] => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return [
      Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)),
      Math.max(0, Math.min(1, (event.clientY - bounds.top) / bounds.height)),
    ];
  };
  const highlighted = useMemo(() => {
    if (!view || view.text.length > 64_000) return null;
    return highlightedSource(view.text, fileLanguage(view.revision.entrypoint));
  }, [view]);
  const region =
    selection?.revisionId === revisionId && selection.page === page
      ? selection.region
      : null;
  const navigation = useMemo(
    () => ({
      path:
        view?.revision.inputKind === "source_tree"
          ? "document.md"
          : (view?.revision.entrypoint.split("/").pop() ?? "document.md"),
      open: async (location: FileLocation) => {
        await adapter.read(location.path);
        setLinkedFile(location);
      },
      image: async (path: string) => {
        const file = await adapter.read(path);
        if (!file.base64 || !file.mime.startsWith("image/"))
          throw new Error("This saved copy does not include the image.");
        return `data:${file.mime};base64,${file.base64}`;
      },
    }),
    [adapter, view],
  );
  const capturedPaths =
    (
      view?.revision.dependencyManifest as
        { files?: Array<{ path: string }> } | undefined
    )?.files?.map((f) => f.path) ?? [];
  return (
    <FileNavigationContext.Provider
      value={view?.revision.inputKind === "source_tree" ? null : navigation}
    >
      <section
        aria-label="Document reader"
        className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg border bg-white dark:bg-neutral-950"
      >
        <header className="flex flex-wrap items-center gap-2 border-b p-3">
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-sm font-semibold">
              {view?.title ?? "Opening document…"}
            </h2>
            <p className="text-[11px] text-gray-500">
              Captured revision {view?.revision.contentHash.slice(0, 12)}
              {busy && " · Loading…"}
            </p>
          </div>
          <button
            className={button}
            onClick={() => setMode("source")}
            aria-pressed={mode === "source"}
          >
            Text / source
          </button>
          <button
            className={button}
            onClick={() => setMode("rendered")}
            aria-pressed={mode === "rendered"}
          >
            Read with math
          </button>
          {view?.revision.inputKind === "pdf" && (
            <button
              className={button}
              onClick={() => setMode("page")}
              aria-pressed={mode === "page"}
            >
              PDF page
            </button>
          )}
        </header>
        <form
          className="flex gap-2 border-b p-2"
          onSubmit={(event) => {
            event.preventDefault();
            void researchClient
              .paperSearch(workspaceId, revisionId, search, 20)
              .then(setHits)
              .catch((e) => onError(workbenchErrorMessage(e)));
          }}
        >
          <input
            aria-label="Search document"
            placeholder="Search this revision"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="min-w-0 flex-1 rounded border bg-transparent px-2 text-sm"
          />
          <button disabled={!search.trim()} className={button}>
            Find
          </button>
          {mode === "page" ? (
            // The interactive PDF reader has its own page controls.
            !interactivePdf && (
              <>
                <button
                  type="button"
                  className={button}
                  disabled={page <= 1}
                  onClick={() => goToPage(page - 1)}
                >
                  Previous
                </button>
                <label className="text-xs">
                  Page{" "}
                  <input
                    aria-label="PDF page"
                    type="number"
                    min={1}
                    max={pageCount ?? undefined}
                    {...pageInput}
                    onKeyDown={(event) => {
                      // Enter goes to the page instead of submitting the search.
                      if (event.key === "Enter") event.preventDefault();
                      pageInput.onKeyDown(event);
                    }}
                    className="w-14 rounded border bg-transparent px-1"
                  />
                </label>
                <button
                  type="button"
                  className={button}
                  disabled={pageCount !== null && page >= pageCount}
                  onClick={() => goToPage(page + 1)}
                >
                  Next
                </button>
              </>
            )
          ) : (
            <>
              <button
                type="button"
                className={button}
                disabled={!start}
                onClick={() => setStart(0)}
              >
                Start
              </button>
              <button
                type="button"
                className={button}
                disabled={!view || view.end >= view.totalBytes}
                onClick={() => {
                  if (view) {
                    setStart(view.end);
                    setSelection(null);
                  }
                }}
              >
                Next text segment
              </button>
            </>
          )}
        </form>
        {hits.length > 0 && (
          <div className="max-h-32 shrink-0 overflow-auto border-b p-2">
            {hits.map((hit) => (
              <button
                key={hit.start}
                className="block w-full truncate p-1 text-left text-xs hover:bg-gray-100 dark:hover:bg-neutral-800"
                onClick={() => {
                  setMode("source");
                  setStart(hit.start);
                }}
              >
                {hit.excerpt}
              </button>
            ))}
          </div>
        )}
        {/* The reading area scrolls as a whole when the window is short, so the
            selection actions below always stay visible. */}
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
          {view?.revision.extraction.status !== "complete" && view && (
            <p className="p-2 text-xs text-amber-700">
              Some text could not be read. Check the original document or PDF
              pages; search may miss passages.
            </p>
          )}
          {view?.revision.inputKind === "pdf" && (
            <p className="px-3 py-2 text-xs text-gray-500">
              Check equations in the PDF page view. Select a region or the whole
              page to attach it.
            </p>
          )}
          {linkedFile && (
            <div className="flex h-[36rem] shrink-0 flex-col border-b">
              <button
                className={`${button} self-start`}
                onClick={() => setLinkedFile(null)}
              >
                Return to document
              </button>
              <div className="min-h-0 flex-1">
                <Suspense fallback={<p>Opening captured file…</p>}>
                  <FileWorkspace
                    key={adapter.id}
                    adapter={adapter}
                    paths={capturedPaths}
                    initial={linkedFile}
                  />
                </Suspense>
              </div>
            </div>
          )}
          {view?.revision.inputKind === "source_tree" && !linkedFile && (
            <div className="p-2">
              <select
                aria-label="Captured source file"
                value=""
                onChange={(e) => setLinkedFile({ path: e.target.value })}
              >
                <option value="">Open a captured file…</option>
                {capturedPaths.map((path) => (
                  <option key={path}>{path}</option>
                ))}
              </select>
            </div>
          )}
          {mode === "page" && interactivePdf && view && (
            <div className="pdf-reader-host flex min-h-[20rem] flex-1 flex-col">
              <Suspense fallback={<p>Loading interactive PDF…</p>}>
                <PdfReader
                  documentKey={`${workspaceId}:${revisionId}`}
                  title={view.title}
                  position={pdfPosition}
                  onPosition={(value) => setPage(value.page)}
                  load={async () => {
                    const file = await adapter.read("");
                    if (!file.base64) throw new Error("PDF bytes unavailable");
                    return file.base64;
                  }}
                  highlight={
                    selection?.page && selection.region
                      ? {
                          page: selection.page,
                          region: selection.region,
                          quote: selection.quote,
                        }
                      : null
                  }
                  onSelection={(value) =>
                    setSelection({
                      revisionId,
                      revisionHash: view.revision.contentHash,
                      start: null,
                      end: null,
                      ...value,
                    })
                  }
                  fallback={
                    <div className="p-3">
                      <button
                        className={button}
                        onClick={() => setInteractivePdf(false)}
                      >
                        Open rendered page images
                      </button>
                    </div>
                  }
                />
              </Suspense>
            </div>
          )}
          <div
            ref={scroll}
            className={`min-h-[12rem] flex-1 overflow-auto p-4 ${mode === "page" && interactivePdf ? "hidden" : ""}`}
            onScroll={(event) => {
              layout.current.scroll = event.currentTarget.scrollTop;
              if (scrollWrite.current) return;
              scrollWrite.current = requestAnimationFrame(() => {
                scrollWrite.current = 0;
                writeLayout();
              });
            }}
          >
            {mode === "source" && (
              <pre
                ref={source}
                tabIndex={0}
                onMouseUp={selectText}
                onKeyUp={selectText}
                className="whitespace-pre-wrap break-words font-mono text-sm leading-7"
              >
                {highlighted ? (
                  <code dangerouslySetInnerHTML={{ __html: highlighted }} />
                ) : (
                  view?.text
                )}
              </pre>
            )}
            {mode === "rendered" && view && (
              <>
                <p className="mb-3 text-xs text-gray-500">
                  Use Text / source to attach an exact selection.
                </p>
                {/\.(csv|tsv)$/i.test(view.revision.entrypoint) ? (
                  <ResearchTablePreview
                    text={view.text}
                    path={view.revision.entrypoint}
                  />
                ) : (
                  <ReportViewer markdown={view.text} />
                )}
              </>
            )}
            {mode === "page" && view?.imageUrl && (
              <div
                className="relative mx-auto w-fit max-w-full touch-none"
                onPointerDown={(event) => {
                  drag.current = pagePoint(event);
                  event.currentTarget.setPointerCapture(event.pointerId);
                }}
                onPointerUp={(event) => {
                  const first = drag.current;
                  drag.current = null;
                  if (!first) return;
                  const last = pagePoint(event);
                  const region: [number, number, number, number] = [
                    Math.min(first[0], last[0]),
                    Math.min(first[1], last[1]),
                    Math.abs(last[0] - first[0]),
                    Math.abs(last[1] - first[1]),
                  ];
                  if (region[2] > 0.005 && region[3] > 0.005)
                    setSelection({
                      revisionId,
                      revisionHash: view.revision.contentHash,
                      page,
                      region,
                      start: null,
                      end: null,
                      quote: "",
                    });
                }}
              >
                <img
                  src={view.imageUrl}
                  alt={`${view.title}, page ${page}`}
                  draggable={false}
                  className="max-w-full select-none"
                />
                {region && (
                  <div
                    className="pointer-events-none absolute border-2 border-blue-600 bg-blue-400/15"
                    style={{
                      left: `${region[0] * 100}%`,
                      top: `${region[1] * 100}%`,
                      width: `${region[2] * 100}%`,
                      height: `${region[3] * 100}%`,
                    }}
                  />
                )}
              </div>
            )}
            {mode === "page" && !interactivePdf && (
              <button
                className={button}
                onClick={() => setInteractivePdf(true)}
              >
                Use interactive PDF
              </button>
            )}
            {mode === "page" && view && (
              <button
                className={`${button} mt-3`}
                onClick={() =>
                  setSelection({
                    revisionId,
                    revisionHash: view.revision.contentHash,
                    page,
                    region: [0, 0, 1, 1],
                    start: null,
                    end: null,
                    quote: "",
                  })
                }
              >
                Select whole page
              </button>
            )}
          </div>
        </div>
        {selection && !readonly && (
          <div className="flex shrink-0 flex-wrap items-center gap-2 border-t bg-blue-50 p-3 dark:bg-blue-950/30">
            <span className="min-w-0 flex-1 truncate text-xs">
              {selection.quote || `Page ${selection.page} region`}
            </span>
            {(
              [
                ["ask", "Ask"],
                ["note", "Add note"],
                ["task", "Create action item"],
                ["revision", "Propose revision"],
              ] as const
            ).map(([action, label]) => (
              <button
                key={action}
                className={button}
                onClick={() => onSelection(selection, action)}
              >
                {label}
              </button>
            ))}
          </div>
        )}
        {annotations.length > 0 && (
          <details className="max-h-40 shrink-0 overflow-auto border-t p-3 text-xs">
            <summary>{annotations.length} saved annotation(s)</summary>
            {annotations.map((annotation) => (
              <button
                key={annotation.id}
                className="mt-2 block w-full text-left"
                onClick={() => {
                  setSelection(annotation.body.selection);
                  if (annotation.body.selection.page) {
                    setMode("page");
                    setPage(pageNumber(annotation.body.selection.page));
                  } else {
                    setMode("source");
                    setStart(annotation.body.selection.start ?? 0);
                  }
                }}
              >
                {annotation.body.body}
              </button>
            ))}
          </details>
        )}
      </section>
    </FileNavigationContext.Provider>
  );
}
