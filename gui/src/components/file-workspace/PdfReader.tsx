import type { MouseEvent as ReactMouseEvent } from "react";
import { pdfAssetOptions } from "../../lib/pdfAssets";
import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  getDocument,
  GlobalWorkerOptions,
  type PDFDocumentProxy,
} from "pdfjs-dist/legacy/build/pdf.mjs";
import {
  EventBus,
  PDFViewer,
  PDFLinkService,
  PDFFindController,
} from "pdfjs-dist/legacy/web/pdf_viewer.mjs";
import workerUrl from "pdfjs-dist/legacy/build/pdf.worker.min.mjs?url";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { safeExternalHref } from "../SafeMarkdownLink";
import "pdfjs-dist/web/pdf_viewer.css";
import "./pdf.css";
GlobalWorkerOptions.workerSrc = workerUrl;

export interface PdfSelection {
  page: number;
  region: [number, number, number, number];
  quote: string;
}
export interface PdfPosition {
  page: number;
  scale?: string;
  rotation?: number;
}
interface Props {
  documentKey: string;
  title: string;
  load: () => Promise<string>;
  initialPage?: number;
  fallback?: ReactNode;
  highlight?: PdfSelection | null;
  onSelection?: (value: PdfSelection) => void;
  onSource?: (point: { page: number; x: number; y: number }) => void;
  position?: PdfPosition;
  onPosition?: (value: PdfPosition) => void;
}
type OutlineEntry = {
  title: string;
  dest: string | unknown[] | null;
  depth: number;
};
export function decodePdf(base64: string): Uint8Array {
  const value = atob(base64);
  return Uint8Array.from(value, (c) => c.charCodeAt(0));
}
function Thumbnail({
  pdf,
  page,
  label,
  onOpen,
}: {
  pdf: PDFDocumentProxy;
  page: number;
  label: string;
  onOpen: () => void;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let live = true;
    let task:
      | ReturnType<Awaited<ReturnType<PDFDocumentProxy["getPage"]>>["render"]>
      | undefined;
    void pdf
      .getPage(page)
      .then((p) => {
        if (!live || !canvas.current) return;
        const viewport = p.getViewport({
          scale: 110 / p.getViewport({ scale: 1 }).width,
        });
        const element = canvas.current;
        element.width = viewport.width;
        element.height = viewport.height;
        task = p.render({ canvas: element, viewport });
        return task.promise;
      })
      .catch(() => {
        /* Main reader reports errors; thumbnail failure is nonfatal. */
      });
    return () => {
      live = false;
      task?.cancel();
    };
  }, [pdf, page]);
  return (
    <button
      type="button"
      className="pdf-thumbnail"
      onClick={onOpen}
      aria-label={`Open page ${label}`}
    >
      <canvas ref={canvas} />
      <span>{label}</span>
    </button>
  );
}

export default function PdfReader(props: Props) {
  const { documentKey, title, fallback, initialPage } = props;
  const container = useRef<HTMLDivElement>(null);
  const pages = useRef<HTMLDivElement>(null);
  const instance = useRef<{
    viewer: PDFViewer;
    bus: EventBus;
    links: PDFLinkService;
  } | null>(null);
  const callbacks = useRef(props);
  callbacks.current = props;
  const [pdf, setPdf] = useState<PDFDocumentProxy | null>(null);
  const [error, setError] = useState("");
  const [page, setPage] = useState(initialPage ?? 1);
  const [pageLabels, setPageLabels] = useState<string[]>([]);
  const [outline, setOutline] = useState<OutlineEntry[]>([]);
  const [sidebar, setSidebar] = useState<"none" | "outline" | "pages">("none");
  const [thumbStart, setThumbStart] = useState(1);
  const searchInput = useRef<HTMLInputElement>(null);
  const [renderEpoch, setRenderEpoch] = useState(0);
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState({ current: 0, total: 0 });
  const [regionMode, setRegionMode] = useState(false);
  const [fallbackMode, setFallbackMode] = useState(false);
  const [selectionNotice, setSelectionNotice] = useState("");
  const [selectionRect, setSelectionRect] = useState<{
    page: number;
    rect: number[];
  } | null>(null);
  const drag = useRef<{ page: number; x: number; y: number } | null>(null);
  const [attempt, setAttempt] = useState(0);
  const go = (number: number) => {
    if (instance.current && pdf)
      instance.current.viewer.currentPageNumber = Math.max(
        1,
        Math.min(pdf.numPages, number),
      );
  };
  useEffect(() => {
    if (fallbackMode || !container.current || !pages.current) return;
    let live = true;
    let loading: ReturnType<typeof getDocument> | undefined;
    setError("");
    setPdf(null);
    setOutline([]);
    setPageLabels([]);
    setSelectionRect(null);
    const bus = new EventBus();
    const links = new PDFLinkService({ eventBus: bus, externalLinkTarget: 0 });
    const find = new PDFFindController({ eventBus: bus, linkService: links });
    const lifetime = new AbortController();
    // PDF.js 6.3 supports abortSignal but omits it from PDFViewerOptions.
    const options: ConstructorParameters<typeof PDFViewer>[0] & {
      abortSignal: AbortSignal;
    } = {
      container: container.current,
      viewer: pages.current,
      eventBus: bus,
      linkService: links,
      findController: find,
      imageResourcesPath: "/pdf-assets/images/",
      textLayerMode: 1,
      annotationMode: 1,
      maxCanvasPixels: 8_000_000,
      abortSignal: lifetime.signal,
    };
    const viewer = new PDFViewer(options);
    links.setViewer(viewer);
    instance.current = { viewer, bus, links };
    let saved: PdfPosition | null = null;
    try {
      saved = JSON.parse(
        localStorage.getItem(`pipeline.pdf.${documentKey}`) ?? "null",
      );
    } catch {
      /* Optional layout. */
    }
    bus.on("pagesinit", () => {
      if (!live) return;
      viewer.currentScaleValue =
        callbacks.current.position?.scale ?? saved?.scale ?? "page-width";
      viewer.pagesRotation =
        callbacks.current.position?.rotation ?? saved?.rotation ?? 0;
      viewer.currentPageNumber = Math.min(
        viewer.pagesCount,
        Math.max(
          1,
          callbacks.current.position?.page ?? initialPage ?? saved?.page ?? 1,
        ),
      );
    });
    const positionChanged = () => {
      if (!live) return;
      const position = {
        page: viewer.currentPageNumber,
        scale: viewer.currentScaleValue,
        rotation: viewer.pagesRotation,
      };
      setPage(position.page);
      const requested = callbacks.current.position?.page;
      if (
        !requested ||
        requested <= viewer.pagesCount ||
        position.page !== viewer.pagesCount
      )
        callbacks.current.onPosition?.(position);
      try {
        localStorage.setItem(
          `pipeline.pdf.${documentKey}`,
          JSON.stringify(position),
        );
      } catch {
        /* Optional layout. */
      }
    };
    bus.on("pagechanging", positionChanged);
    bus.on("scalechanging", positionChanged);
    bus.on("rotationchanging", positionChanged);
    bus.on(
      "updatefindmatchescount",
      (event: { matchesCount: { current: number; total: number } }) => {
        if (live) setMatches(event.matchesCount);
      },
    );
    bus.on("pagerendered", (event: { error?: unknown }) => {
      if (live && event.error) setError(String(event.error));
      else if (live) setRenderEpoch((n) => n + 1);
    });
    void callbacks.current
      .load()
      .then(async (encoded) => {
        if (!live) return;
        loading = getDocument({
          ...pdfAssetOptions,
          data: decodePdf(encoded),
          useSystemFonts: true,
          isOffscreenCanvasSupported: false,
        });
        const document = await loading.promise;
        if (!live) return;
        if (document.numPages > 5000)
          throw new Error(
            "This PDF exceeds the interactive viewer's 5,000-page limit.",
          );
        setPdf(document);
        links.setDocument(document);
        viewer.setDocument(document);
        const [labels, tree] = await Promise.all([
          document.getPageLabels(),
          document.getOutline(),
        ]);
        if (!live) return;
        setPageLabels(labels ?? []);
        const flattened: OutlineEntry[] = [];
        const walk = (items: NonNullable<typeof tree>, depth: number) => {
          if (depth > 12 || flattened.length >= 2000) return;
          for (const item of items) {
            if (flattened.length >= 2000) break;
            flattened.push({ title: item.title, dest: item.dest, depth });
            walk(item.items, depth + 1);
          }
        };
        if (tree) walk(tree, 0);
        setOutline(flattened);
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
      instance.current = null;
      lifetime.abort();
      viewer.cleanup();
      // setDocument(null) is the implemented reset API; its declaration is narrower.
      (
        viewer as { setDocument(document: PDFDocumentProxy | null): void }
      ).setDocument(null);
      links.setDocument(null);
      void loading?.destroy().catch(() => {
        /* Teardown can reject after a worker has already stopped. */
      });
    };
  }, [documentKey, attempt, fallbackMode]);
  useEffect(() => {
    if (initialPage) go(initialPage);
  }, [initialPage, pdf]);
  useEffect(() => {
    const viewer = instance.current?.viewer;
    if (!viewer || !pdf || !props.position) return;
    const position = props.position;
    if (position.page !== viewer.currentPageNumber)
      viewer.currentPageNumber = Math.max(
        1,
        Math.min(pdf.numPages, position.page),
      );
    if (position.scale && position.scale !== viewer.currentScaleValue)
      viewer.currentScaleValue = position.scale;
    if (
      position.rotation !== undefined &&
      position.rotation !== viewer.pagesRotation
    )
      viewer.pagesRotation = position.rotation;
  }, [props.position, pdf]);
  const search = (previous = false, again = false) =>
    instance.current?.bus.dispatch("find", {
      source: instance.current,
      type: again ? "again" : "",
      query,
      phraseSearch: true,
      caseSensitive: false,
      entireWord: false,
      highlightAll: true,
      findPrevious: previous,
      matchDiacritics: false,
    });
  const pageAt = (target: EventTarget | null) =>
    target instanceof Element
      ? target.closest<HTMLElement>(".page[data-page-number]")
      : null;
  const selection = (
    element: HTMLElement,
    rect: { left: number; top: number; right: number; bottom: number },
    quote: string,
  ) => {
    const pageNumber = Number(element.dataset.pageNumber);
    const pageView = instance.current?.viewer.getPageView(pageNumber - 1);
    if (!pageView?.viewport) return;
    const bounds = element.getBoundingClientRect();
    const viewport = pageView.viewport;
    const corners = [
      [rect.left, rect.top],
      [rect.right, rect.bottom],
    ].map(([x, y]) =>
      viewport.convertToPdfPoint(
        ((x - bounds.left) * viewport.width) / bounds.width,
        ((y - bounds.top) * viewport.height) / bounds.height,
      ),
    );
    const [x0, y0, x1, y1] = viewport.viewBox;
    const left = Math.max(
      0,
      (Math.min(corners[0][0], corners[1][0]) - x0) / (x1 - x0),
    );
    const top = Math.max(
      0,
      (y1 - Math.max(corners[0][1], corners[1][1])) / (y1 - y0),
    );
    const width = Math.min(
      1 - left,
      Math.abs(corners[1][0] - corners[0][0]) / (x1 - x0),
    );
    const height = Math.min(
      1 - top,
      Math.abs(corners[1][1] - corners[0][1]) / (y1 - y0),
    );
    if (width <= 0 || height <= 0) return;
    setSelectionRect({
      page: pageNumber,
      rect: [
        (rect.left - bounds.left) / bounds.width,
        (rect.top - bounds.top) / bounds.height,
        (rect.right - rect.left) / bounds.width,
        (rect.bottom - rect.top) / bounds.height,
      ],
    });
    callbacks.current.onSelection?.({
      page: pageNumber,
      region: [left, top, width, height],
      quote: quote.slice(0, 16000),
    });
  };
  useEffect(() => {
    pages.current
      ?.querySelectorAll(".pdf-research-selection")
      .forEach((e) => e.remove());
    if (!selectionRect) return;
    const pageElement = pages.current?.querySelector<HTMLElement>(
      `.page[data-page-number="${selectionRect.page}"]`,
    );
    if (!pageElement) return;
    const overlay = document.createElement("div");
    overlay.className = "pdf-research-selection";
    const [left, top, width, height] = selectionRect.rect;
    Object.assign(overlay.style, {
      left: `${left * 100}%`,
      top: `${top * 100}%`,
      width: `${width * 100}%`,
      height: `${height * 100}%`,
    });
    pageElement.append(overlay);
  }, [selectionRect]);
  useEffect(() => {
    const highlight = props.highlight;
    if (!highlight) return;
    const element = pages.current?.querySelector<HTMLElement>(
      `.page[data-page-number="${highlight.page}"]`,
    );
    const viewport = instance.current?.viewer.getPageView(
      highlight.page - 1,
    )?.viewport;
    if (!element || !viewport) return;
    const [x0, y0, x1, y1] = viewport.viewBox;
    const [left, top, width, height] = highlight.region;
    const rect = viewport.convertToViewportRectangle([
      x0 + left * (x1 - x0),
      y1 - (top + height) * (y1 - y0),
      x0 + (left + width) * (x1 - x0),
      y1 - top * (y1 - y0),
    ]);
    pages.current
      ?.querySelectorAll(".pdf-research-selection")
      .forEach((e) => e.remove());
    const overlay = document.createElement("div");
    overlay.className = "pdf-research-selection";
    Object.assign(overlay.style, {
      left: `${(100 * Math.min(rect[0], rect[2])) / viewport.width}%`,
      top: `${(100 * Math.min(rect[1], rect[3])) / viewport.height}%`,
      width: `${(100 * Math.abs(rect[2] - rect[0])) / viewport.width}%`,
      height: `${(100 * Math.abs(rect[3] - rect[1])) / viewport.height}%`,
    });
    element.append(overlay);
  }, [props.highlight, renderEpoch]);
  const openLink = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (event.button === 2) return;
    const anchor =
      event.target instanceof Element ? event.target.closest("a") : null;
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    if (href.startsWith("#")) return;
    event.preventDefault();
    event.stopPropagation();
    const safe = safeExternalHref(href);
    if (safe) void openExternal(safe).catch((e) => setError(String(e)));
  };
  if (fallbackMode)
    return (
      <div className="file-workspace">
        <div className="file-toolbar">
          <button onClick={() => setFallbackMode(false)}>
            Return to interactive PDF
          </button>
        </div>
        {fallback}
      </div>
    );
  return (
    <section
      className="pdf-reader"
      aria-label={`${title} PDF reader`}
      onKeyDownCapture={(event) => {
        if (
          (event.metaKey || event.ctrlKey) &&
          event.key.toLowerCase() === "f"
        ) {
          event.preventDefault();
          event.stopPropagation();
          searchInput.current?.focus();
          searchInput.current?.select();
        }
      }}
    >
      <div className="file-toolbar">
        <button
          onClick={() => setSidebar(sidebar === "pages" ? "none" : "pages")}
        >
          Thumbnails
        </button>
        <button
          onClick={() => setSidebar(sidebar === "outline" ? "none" : "outline")}
        >
          Outline
        </button>
        <button
          disabled={!pdf || page <= 1}
          onClick={() => go(page - 1)}
          aria-label="Previous PDF page"
        >
          Previous
        </button>
        <label>
          Page{" "}
          <input
            aria-label="PDF page number"
            className="file-line-input"
            value={pageLabels[page - 1] ?? page}
            onChange={(e) => {
              const labelIndex = pageLabels.indexOf(e.target.value);
              const n =
                labelIndex >= 0 ? labelIndex + 1 : Number(e.target.value);
              if (n > 0) go(n);
            }}
          />
        </label>
        <span>of {pdf?.numPages ?? "…"}</span>
        <button
          disabled={!pdf || page >= pdf.numPages}
          onClick={() => go(page + 1)}
          aria-label="Next PDF page"
        >
          Next
        </button>
        <button
          aria-label="Zoom out"
          onClick={() => {
            const v = instance.current?.viewer;
            if (v) v.currentScale = Math.max(0.25, v.currentScale / 1.2);
          }}
        >
          −
        </button>
        <button
          aria-label="Zoom in"
          onClick={() => {
            const v = instance.current?.viewer;
            if (v) v.currentScale = Math.min(4, v.currentScale * 1.2);
          }}
        >
          +
        </button>
        <button
          onClick={() => {
            if (instance.current)
              instance.current.viewer.currentScaleValue = "page-width";
          }}
        >
          Fit width
        </button>
        <button
          onClick={() => {
            if (instance.current)
              instance.current.viewer.currentScaleValue = "page-fit";
          }}
        >
          Fit page
        </button>
        <button
          onClick={() => {
            const v = instance.current?.viewer;
            if (v) {
              setSelectionRect(null);
              v.pagesRotation = (v.pagesRotation + 90) % 360;
            }
          }}
        >
          Rotate
        </button>
        {props.onSelection && (
          <button
            aria-pressed={regionMode}
            onClick={() => setRegionMode(!regionMode)}
          >
            {regionMode ? "Select text" : "Select region"}
          </button>
        )}
        {fallback && (
          <button onClick={() => setFallbackMode(true)}>
            Page image fallback
          </button>
        )}
      </div>
      <form
        className="file-toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          search();
        }}
      >
        <input
          ref={searchInput}
          aria-label="Find in PDF"
          value={query}
          placeholder="Find text in PDF"
          onChange={(e) => setQuery(e.target.value)}
        />
        <button disabled={!query || !pdf}>Find</button>
        <button
          type="button"
          disabled={!query}
          onClick={() => search(true, true)}
        >
          Previous match
        </button>
        <button
          type="button"
          disabled={!query}
          onClick={() => search(false, true)}
        >
          Next match
        </button>
        <span role="status">
          {matches.current} / {matches.total} matches
        </span>
      </form>
      {error && (
        <div role="alert" className="file-error">
          {error}{" "}
          <button onClick={() => setAttempt((n) => n + 1)}>Retry</button>
          {fallback && (
            <button onClick={() => setFallbackMode(true)}>
              Use page images
            </button>
          )}
        </div>
      )}
      {!pdf && !error && <p className="file-status">Opening PDF…</p>}
      {selectionNotice && (
        <p role="status" className="file-status">
          {selectionNotice}
        </p>
      )}
      <div className="pdf-layout">
        {sidebar !== "none" && (
          <aside className="pdf-sidebar">
            {sidebar === "outline" ? (
              <>
                {outline.length === 0 && <p>No document outline.</p>}
                {outline.map((entry, index) => (
                  <button
                    key={index}
                    style={{ paddingLeft: 8 + entry.depth * 10 }}
                    onClick={() => {
                      if (entry.dest)
                        void instance.current?.links.goToDestination(
                          entry.dest,
                        );
                    }}
                  >
                    {entry.title}
                  </button>
                ))}
              </>
            ) : (
              pdf && (
                <>
                  <div className="file-toolbar">
                    <button
                      disabled={thumbStart <= 1}
                      onClick={() => setThumbStart((n) => Math.max(1, n - 12))}
                    >
                      Previous 12
                    </button>
                    <button
                      disabled={thumbStart + 12 > pdf.numPages}
                      onClick={() => setThumbStart((n) => n + 12)}
                    >
                      Next 12
                    </button>
                  </div>
                  {Array.from(
                    { length: Math.min(12, pdf.numPages - thumbStart + 1) },
                    (_, i) => (
                      <Thumbnail
                        key={thumbStart + i}
                        pdf={pdf}
                        page={thumbStart + i}
                        label={
                          pageLabels[thumbStart + i - 1] ??
                          String(thumbStart + i)
                        }
                        onOpen={() => go(thumbStart + i)}
                      />
                    ),
                  )}
                </>
              )
            )}
          </aside>
        )}
        <div className="pdf-positioner">
          <div
            ref={container}
            className={`pdf-scroll-container${regionMode ? " pdf-region-mode" : ""}`}
            tabIndex={0}
            onClickCapture={openLink}
            onAuxClickCapture={openLink}
            onDoubleClick={(event) => {
              if (!props.onSource) return;
              const element = pageAt(event.target);
              if (!element) return;
              const n = Number(element.dataset.pageNumber);
              const v = instance.current?.viewer.getPageView(n - 1)?.viewport;
              if (!v) return;
              const bounds = element.getBoundingClientRect();
              const [x, y] = v.convertToPdfPoint(
                ((event.clientX - bounds.left) * v.width) / bounds.width,
                ((event.clientY - bounds.top) * v.height) / bounds.height,
              );
              props.onSource({
                page: n,
                x: x - v.viewBox[0],
                y: v.viewBox[3] - y,
              });
            }}
            onPointerDown={(event) => {
              if (!regionMode) return;
              const element = pageAt(event.target);
              if (!element) return;
              event.preventDefault();
              drag.current = {
                page: Number(element.dataset.pageNumber),
                x: event.clientX,
                y: event.clientY,
              };
              event.currentTarget.setPointerCapture(event.pointerId);
            }}
            onPointerUp={(event) => {
              setSelectionNotice("");
              if (regionMode && drag.current) {
                const start = drag.current;
                drag.current = null;
                const element = pages.current?.querySelector<HTMLElement>(
                  `.page[data-page-number="${start.page}"]`,
                );
                if (element)
                  selection(
                    element,
                    {
                      left: Math.min(start.x, event.clientX),
                      top: Math.min(start.y, event.clientY),
                      right: Math.max(start.x, event.clientX),
                      bottom: Math.max(start.y, event.clientY),
                    },
                    "",
                  );
                return;
              }
              const selected = window.getSelection();
              if (!selected?.rangeCount || selected.isCollapsed) return;
              const range = selected.getRangeAt(0);
              const first = pageAt(range.startContainer.parentElement);
              const last = pageAt(range.endContainer.parentElement);
              if (!first || !last) return;
              if (first !== last) {
                setSelectionNotice(
                  "Select a passage within one page to attach its exact location.",
                );
                return;
              }
              selection(
                first,
                range.getBoundingClientRect(),
                selected.toString(),
              );
            }}
          >
            <div ref={pages} className="pdfViewer" />
          </div>
        </div>
      </div>
    </section>
  );
}
