import SplitView from "../SplitView";
import { useEffect, useRef, useState } from "react";
import { type PDFDocumentProxy } from "pdfjs-dist/legacy/build/pdf.mjs";
import PdfReader, { type PdfPosition } from "./PdfReader";
import { workspaceFileAdapter } from "../../lib/fileWorkspaceClient";
interface Document {
  id: string;
  title: string;
}
export default function PdfComparison({
  workspaceId,
  primary,
  secondary,
  page,
  onSource,
}: {
  workspaceId: string;
  primary: Document;
  secondary?: Document;
  page?: number;
  onSource?: (point: { page: number; x: number; y: number }) => void;
}) {
  const [linked, setLinked] = useState(true);
  const [position, setPosition] = useState<PdfPosition>({ page: page ?? 1 });
  const [changes, setChanges] = useState<number[]>([]);
  const [progress, setProgress] = useState("");
  const [comparing, setComparing] = useState(false);
  const generation = useRef(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      generation.current++;
    };
  }, []);
  useEffect(() => {
    if (page) setPosition((old) => ({ ...old, page }));
  }, [page]);
  const [documents, setDocuments] = useState(
    new Map<string, PDFDocumentProxy>(),
  );
  const digests = useRef(new WeakMap<PDFDocumentProxy, Map<number, string>>());
  const attach = (id: string, doc: PDFDocumentProxy | null) => {
    setDocuments((previous) => {
      const next = new Map(previous);
      if (doc) next.set(id, doc);
      else next.delete(id);
      return next;
    });
  };
  useEffect(() => {
    generation.current++;
    setChanges([]);
    setProgress("");
    setComparing(false);
  }, [primary.id, secondary?.id]);
  const compare = async () => {
    if (!secondary) return;
    const run = ++generation.current;
    setComparing(true);
    setChanges([]);

    const image = async (pdf: PDFDocumentProxy, page: number) => {
      if (page > pdf.numPages) return "missing";
      const cache = digests.current.get(pdf) ?? new Map<number, string>();
      digests.current.set(pdf, cache);
      const cached = cache.get(page);
      if (cached) return cached;
      const p = await pdf.getPage(page);
      const size = p.getViewport({ scale: 1 });
      const scale = Math.min(320 / size.width, 480 / size.height);
      const viewport = p.getViewport({ scale });
      const canvas = document.createElement("canvas");
      canvas.width = Math.ceil(viewport.width);
      canvas.height = Math.ceil(viewport.height);
      try {
        await p.render({ canvas, viewport }).promise;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Page comparison canvas unavailable");
        const pixels = context.getImageData(
          0,
          0,
          canvas.width,
          canvas.height,
        ).data;
        const digest = await crypto.subtle.digest("SHA-256", pixels);
        const fingerprint = `${canvas.width}:${canvas.height}:${Array.from(new Uint8Array(digest)).join(",")}`;
        cache.set(page, fingerprint);
        return fingerprint;
      } finally {
        canvas.width = 0;
        canvas.height = 0;
        // The reader owns the page and may still be painting it.
      }
    };
    try {
      const a = documents.get(primary.id),
        b = documents.get(secondary.id);
      if (!a || !b)
        throw new Error("Wait for both PDFs to open, then compare again.");
      const count = Math.max(a.numPages, b.numPages);
      if (count > 5000)
        throw new Error(
          "PDF comparison supports at most 5,000 pages per document.",
        );
      const found: number[] = [];
      for (let n = 1; n <= count; n++) {
        if (run !== generation.current || !mounted.current) return;
        const [left, right] = await Promise.all([image(a, n), image(b, n)]);
        if (run !== generation.current || !mounted.current) return;
        if (left !== right) found.push(n);
        setProgress(`Compared ${n} of ${count} pages`);
        setChanges([...found]);
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() => resolve()),
        );
      }
      if (run === generation.current)
        setProgress(
          `Compared ${count} pages at thumbnail resolution; ${found.length} pages differ. Inspect full-size pages for details.`,
        );
    } catch (e) {
      if (run === generation.current && mounted.current) setProgress(String(e));
    } finally {
      if (run === generation.current && mounted.current) setComparing(false);
    }
  };
  const changePage = (direction: number) => {
    const next =
      direction > 0
        ? (changes.find((n) => n > position.page) ?? changes[0])
        : ([...changes].reverse().find((n) => n < position.page) ??
          changes[changes.length - 1]);
    if (next) setPosition((old) => ({ ...old, page: next }));
  };
  return (
    <div className="file-workspace">
      {secondary && (
        <div className="file-toolbar">
          <label>
            <input
              type="checkbox"
              checked={linked}
              onChange={(e) => setLinked(e.target.checked)}
            />{" "}
            Synchronize page, zoom and rotation
          </label>
          <button
            disabled={
              comparing ||
              !documents.has(primary.id) ||
              !documents.has(secondary.id)
            }
            onClick={() => void compare()}
          >
            Find changed pages
          </button>
          {comparing && (
            <button
              onClick={() => {
                generation.current++;
                setComparing(false);
                setProgress("Comparison stopped");
              }}
            >
              Stop comparison
            </button>
          )}
          <button disabled={!changes.length} onClick={() => changePage(-1)}>
            Previous changed page
          </button>
          <button disabled={!changes.length} onClick={() => changePage(1)}>
            Next changed page
          </button>
        </div>
      )}
      {progress && (
        <p role="status" className="file-status">
          {progress}
        </p>
      )}
      {secondary ? (
        <SplitView
          storageKey={`pipeline.files.pdfSplit.${workspaceId}`}
          firstLabel={`Primary PDF: ${primary.title}`}
          secondLabel={`Second PDF: ${secondary.title}`}
          first={
            <PdfReader
              documentKey={`${workspaceId}:${primary.id}`}
              title={primary.title}
              initialPage={page}
              capture={workspaceFileAdapter({
                workspaceId,
                revisionId: primary.id,
              })}
              onDocument={(doc) => attach(primary.id, doc)}
              position={position}
              onPosition={setPosition}
              onSource={onSource}
            />
          }
          second={
            <PdfReader
              documentKey={`${workspaceId}:${secondary.id}`}
              title={secondary.title}
              capture={workspaceFileAdapter({
                workspaceId,
                revisionId: secondary.id,
              })}
              onDocument={(doc) => attach(secondary.id, doc)}
              position={linked ? position : undefined}
              onPosition={linked ? setPosition : undefined}
            />
          }
        />
      ) : (
        <div className="file-panes">
          <PdfReader
            documentKey={`${workspaceId}:${primary.id}`}
            title={primary.title}
            initialPage={page}
            capture={workspaceFileAdapter({
              workspaceId,
              revisionId: primary.id,
            })}
            onDocument={(doc) => attach(primary.id, doc)}
            position={position}
            onPosition={setPosition}
            onSource={onSource}
          />
        </div>
      )}
    </div>
  );
}
