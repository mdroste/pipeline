import { useEffect, useRef, useState } from "react";
import { artifactClient } from "../../lib/artifactClient";
import type {
  ArtifactEntry,
  ArtifactContent,
  PdfArtifactPage,
  PdfArtifactPagePreview,
} from "../../lib/artifactTypes";
import { formatBytes } from "./format";
export function PdfView({
  runId,
  entry,
  content,
}: {
  runId: string;
  entry: ArtifactEntry | null;
  content: ArtifactContent;
}) {
  const [page, setPage] = useState(1);
  const [preview, setPreview] = useState<PdfArtifactPage | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const previewCache = useRef(new Map<number, PdfArtifactPagePreview>());
  const relPath = entry?.rel_path ?? "";

  useEffect(() => {
    if (!relPath) return;
    const cached = previewCache.current.get(page);
    if (cached) {
      // Refresh LRU order without changing the cached value.
      previewCache.current.delete(page);
      previewCache.current.set(page, cached);
      setPreview(cached);
      setPreviewError(null);
      return;
    }
    let live = true;
    setPreview(null);
    setPreviewError(null);
    artifactClient
      .pdfPage(runId, relPath, page)
      .then((result) => {
        if (!live) return;
        const cache = previewCache.current;
        cache.set(result.page, result);
        if (result.prefetched_next) {
          cache.set(result.prefetched_next.page, result.prefetched_next);
        }
        while (cache.size > 6) {
          const oldest = cache.keys().next().value;
          if (oldest === undefined) break;
          cache.delete(oldest);
        }
        setPreview(result);
      })
      .catch((caught) => {
        if (!live) return;
        const message =
          caught instanceof Error ? caught.message : String(caught);
        setPreviewError(message);
      });
    return () => {
      live = false;
    };
  }, [attempt, page, relPath, runId]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div
        className="flex shrink-0 items-center justify-between gap-3 border-b border-gray-200
                      bg-white px-4 py-2 dark:border-gray-800 dark:bg-gray-950"
      >
        <div className="min-w-0">
          <p className="truncate text-xs font-medium text-gray-700 dark:text-gray-200">
            {entry?.label ?? "PDF artifact"}
          </p>
          <p className="text-[11px] text-gray-500 dark:text-gray-400">
            {formatBytes(content.bytes)} · rendered in Pipeline
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button
            type="button"
            onClick={() => setPage((value) => Math.max(1, value - 1))}
            disabled={!preview?.has_previous}
            aria-label="Previous PDF page"
            className="rounded border border-gray-300 px-2 py-1 text-xs text-gray-600 hover:bg-gray-50
                       disabled:cursor-default disabled:opacity-40 dark:border-gray-700 dark:text-gray-300
                       dark:hover:bg-gray-800"
          >
            Previous
          </button>
          <span className="min-w-14 text-center text-xs text-gray-600 dark:text-gray-300">
            Page {page}
          </span>
          <button
            type="button"
            onClick={() => setPage((value) => value + 1)}
            disabled={!preview?.has_next}
            aria-label="Next PDF page"
            className="rounded border border-gray-300 px-2 py-1 text-xs text-gray-600 hover:bg-gray-50
                       disabled:cursor-default disabled:opacity-40 dark:border-gray-700 dark:text-gray-300
                       dark:hover:bg-gray-800"
          >
            Next
          </button>
        </div>
      </div>
      <div className="flex-1 min-h-0 overflow-auto bg-gray-50 p-4 dark:bg-gray-900">
        {previewError ? (
          <div
            role="alert"
            className="mx-auto mt-8 max-w-lg rounded-lg border border-red-200 bg-red-50 p-4
                                       text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300"
          >
            <p className="font-medium">
              Could not render this PDF in Pipeline.
            </p>
            <p className="mt-1 text-xs">{previewError}</p>
            <button
              type="button"
              onClick={() => {
                previewCache.current.delete(page);
                setAttempt((value) => value + 1);
              }}
              className="mt-3 rounded border border-red-300 px-2 py-1 text-xs hover:bg-red-100
                         dark:border-red-800 dark:hover:bg-red-900/40"
            >
              Retry
            </button>
          </div>
        ) : preview ? (
          <img
            src={`data:image/jpeg;base64,${preview.base64}`}
            alt={`${entry?.label ?? "PDF artifact"}, page ${preview.page}`}
            className="mx-auto h-auto max-w-full shadow-sm"
          />
        ) : (
          <div className="flex h-full items-center justify-center text-gray-400">
            <div className="animate-spin h-5 w-5 rounded-full border-2 border-gray-300 border-t-gray-600" />
          </div>
        )}
      </div>
    </div>
  );
}
