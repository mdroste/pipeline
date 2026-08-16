// Run-artifact explorer: a manifest-driven tree beside a kind-dispatched
// viewer. Source bytes arrive through validated, size-capped Tauri commands —
// never file:// URLs. PDFs are rendered to bounded page images in Rust, so
// previews behave consistently across WKWebView / WebView2 / WebKitGTK.

import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import hljs from "highlight.js/lib/common";
import "highlight.js/styles/github.css";
import ReportViewer from "./ReportViewer";
import ResizeHandle from "./ResizeHandle";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";

interface ArtifactEntry {
  rel_path: string;
  label: string;
  kind: string;
  bytes: number;
  sha256: string;
  group: string;
}

interface PageArtifactIndex {
  count: number;
  digit_width: number;
  extension: string;
  total_bytes: number;
}

export interface RunManifest {
  artifact_schema_version?: number;
  run_id: string;
  created: string;
  input_path: string;
  input_mode: string;
  profile_id?: string;
  profile_name: string;
  specialist_catalog_revision?: string;
  provider: string;
  artifacts: ArtifactEntry[];
  page_artifacts?: PageArtifactIndex | null;
  status?: string;
  duration_secs?: number;
  usage?: {
    input_tokens?: number;
    output_tokens?: number;
    cached_input_tokens?: number;
    cache_write_input_tokens?: number;
  };
  title?: string;
}

export interface ArtifactContent {
  kind: string;
  bytes: number;
  text: string | null;
  base64: string | null;
  truncated: boolean;
  abs_path: string;
}

interface PdfArtifactPagePreview {
  page: number;
  has_previous: boolean;
  has_next: boolean;
  base64: string;
}

interface PdfArtifactPage extends PdfArtifactPagePreview {
  prefetched_next?: PdfArtifactPagePreview | null;
}

interface DocumentRepresentation {
  format: string;
  content: unknown;
}

interface DocumentNode {
  id: string;
  kind: string;
  order: number;
  page?: number;
  parent_id?: string;
  label?: string;
  number?: string;
  text: string;
  asset_ids: string[];
  representations: DocumentRepresentation[];
  provenance: {
    origin_id: string;
    method: string;
    confidence?: number;
  };
}

interface DocumentAsset {
  id: string;
  kind: string;
  label: string;
  rel_path: string;
  media_type: string;
  page?: number;
  width?: number;
  height?: number;
  provenance: {
    origin_id: string;
    method: string;
    confidence?: number;
  };
}

interface DocumentBundle {
  schema_version: string;
  bundle_id: string;
  source_kind: string;
  origins: Array<{ id: string; kind: string; path: string; role: string }>;
  pages: Array<{
    number: number;
    label: string;
    asset_id?: string;
    width?: number;
    height?: number;
  }>;
  nodes: DocumentNode[];
  assets: DocumentAsset[];
  links?: Array<{ from_id: string; to_id: string; kind: string }>;
  extraction: { method: string; source_path: string; paper_hash: string };
  quality: Array<{ severity: string; scope: string; message: string }>;
}

interface Props {
  runId: string;
  /** Rendered if the manifest can't be loaded (run dir missing, etc.). */
  fallbackMarkdown: string;
  /** Load only the manifest until the user explicitly chooses an artifact. */
  deferInitialArtifact?: boolean;
  /** Background work started by the report workspace before Sources opens. */
  preload?: ArtifactExplorerPreload | null;
  /** External navigation request, for example from an evidence-linked issue. */
  selectionRequest?: ArtifactSelectionRequest | null;
}

export interface ArtifactSelectionRequest {
  key: number;
  page?: number;
  relPath?: string;
}

export type ArtifactSelectionTarget = Omit<ArtifactSelectionRequest, "key">;

export interface ArtifactExplorerPreload {
  runId: string;
  manifest: Promise<RunManifest>;
  readableDocument: Promise<ArtifactContent>;
}

/** Skip syntax highlighting beyond this size — highlightAuto gets slow. */
const MAX_HIGHLIGHT_CHARS = 200_000;
const MAX_CSV_ROWS = 200;
const MAX_INDEXED_PAGES = 5_000;
const PAGE_SELECTION_PREFIX = "__page__:";

const EXT_LANGUAGE: Record<string, string> = {
  rs: "rust", py: "python", ts: "typescript", tsx: "typescript", js: "javascript",
  jsx: "javascript", r: "r", json: "json", yaml: "yaml", yml: "yaml", toml: "ini",
  sh: "bash", sql: "sql", c: "c", cpp: "cpp", h: "c", java: "java", go: "go",
  rb: "ruby", css: "css", html: "xml", md: "markdown",
};

const IMAGE_MIME: Record<string, string> = {
  png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif",
  webp: "image/webp", svg: "image/svg+xml",
};

function ext(relPath: string): string {
  const dot = relPath.lastIndexOf(".");
  return dot === -1 ? "" : relPath.slice(dot + 1).toLowerCase();
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function pageSelection(page: number): string {
  return `${PAGE_SELECTION_PREFIX}${page}`;
}

function selectedPage(selection: string): number | null {
  if (!selection.startsWith(PAGE_SELECTION_PREFIX)) return null;
  const page = Number(selection.slice(PAGE_SELECTION_PREFIX.length));
  return Number.isInteger(page) && page > 0 ? page : null;
}

function indexedPageForPath(
  relPath: string,
  index?: PageArtifactIndex | null,
): number | null {
  if (!index) return null;
  const match = relPath.match(/^artifacts\/pages\/page-(\d+)\.(?:jpe?g|png)$/i);
  if (!match) return null;
  const page = Number(match[1]);
  const count = Number.isSafeInteger(index.count)
    ? Math.min(Math.max(index.count, 0), MAX_INDEXED_PAGES)
    : 0;
  return Number.isInteger(page) && page > 0 && page <= count ? page : null;
}

function CodeView({ text, relPath }: { text: string; relPath: string }) {
  const lang = EXT_LANGUAGE[ext(relPath)];
  const html = useMemo(() => {
    if (text.length > MAX_HIGHLIGHT_CHARS) return null;
    try {
      return lang && hljs.getLanguage(lang)
        ? hljs.highlight(text, { language: lang }).value
        : hljs.highlightAuto(text).value;
    } catch {
      return null;
    }
  }, [lang, text]);
  return (
    <pre className="p-4 text-xs font-mono leading-relaxed whitespace-pre overflow-auto bg-white rounded border border-gray-200 dark:border-gray-700">
      {html !== null ? (
        <code dangerouslySetInnerHTML={{ __html: html }} />
      ) : (
        <code>{text}</code>
      )}
    </pre>
  );
}

function CsvView({ text, relPath }: { text: string; relPath: string }) {
  const delim = ext(relPath) === "tsv" ? "\t" : ",";
  // Naive split — quoted delimiters aren't handled; fine for a preview.
  const lines = text.split(/\r?\n/).filter((l) => l.length > 0);
  const rows = lines.slice(0, MAX_CSV_ROWS).map((l) => l.split(delim));
  return (
    <div className="overflow-auto">
      <table className="text-xs border-collapse">
        <tbody>
          {rows.map((cells, i) => (
            <tr key={i} className={i === 0 ? "font-semibold bg-gray-50 dark:bg-gray-800" : ""}>
              {cells.map((c, j) => (
                <td key={j} className="border border-gray-200 dark:border-gray-700 px-2 py-1 whitespace-nowrap">
                  {c}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {lines.length > MAX_CSV_ROWS && (
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
          Showing first {MAX_CSV_ROWS} of {lines.length} rows.
        </p>
      )}
    </div>
  );
}

function BinaryCard({
  entry,
  content,
}: {
  entry: ArtifactEntry | null;
  content: ArtifactContent;
}) {
  const [openError, setOpenError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);

  const openInSystemViewer = async () => {
    setOpenError(null);
    setOpening(true);
    try {
      await openExternal(content.abs_path);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setOpenError(`Could not open this artifact in the system viewer: ${message}`);
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="max-w-sm mx-auto mt-12 p-5 rounded-lg border border-gray-200 dark:border-gray-700 text-center">
      <p className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
        {entry?.label ?? "Binary file"}
      </p>
      <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">
        {formatBytes(content.bytes)} — not previewable in the app.
      </p>
      <button
        type="button"
        onClick={openInSystemViewer}
        disabled={opening}
        className="py-1.5 px-4 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors
                   disabled:cursor-wait disabled:opacity-50"
      >
        {opening ? "Opening…" : "Open in system viewer"}
      </button>
      {openError && (
        <p role="alert" className="mt-3 text-xs text-red-700 dark:text-red-400">
          {openError}
        </p>
      )}
    </div>
  );
}

function PdfView({
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
    invoke<PdfArtifactPage>("read_pdf_artifact_page", { runId, relPath, page })
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
        const message = caught instanceof Error ? caught.message : String(caught);
        setPreviewError(message);
      });
    return () => {
      live = false;
    };
  }, [attempt, page, relPath, runId]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center justify-between gap-3 border-b border-gray-200
                      bg-white px-4 py-2 dark:border-gray-800 dark:bg-gray-950">
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
          <div role="alert" className="mx-auto mt-8 max-w-lg rounded-lg border border-red-200 bg-red-50 p-4
                                       text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">
            <p className="font-medium">Could not render this PDF in Pipeline.</p>
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

function formatRepresentation(value: unknown): string {
  const text = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  if (!text) return "";
  return text.length > 4_000 ? `${text.slice(0, 4_000)}\n…` : text;
}

const STRUCTURAL_KINDS = new Set([
  "section", "equation", "table", "figure", "theorem", "proposition", "lemma", "corollary",
]);
const INITIAL_BUNDLE_NODES = 100;

function equationNumberText(text: string): string | null {
  let value = text.trim();
  while (value.length >= 2 && value.startsWith("$") && value.endsWith("$")) {
    value = value.slice(1, -1).trim();
  }
  if (value.startsWith("\\(") && value.endsWith("\\)")) {
    value = value.slice(2, -2).trim();
  } else if (value.startsWith("\\[") && value.endsWith("\\]")) {
    value = value.slice(2, -2).trim();
  }
  const tag = /^\\tag\*?\{([^{}]{1,40})\}$/.exec(value);
  if (tag) value = tag[1].trim();
  if (
    ((value.startsWith("(") && value.endsWith(")"))
      || (value.startsWith("[") && value.endsWith("]")))
    && value.length >= 2
  ) {
    value = value.slice(1, -1).trim();
  }
  return /^[a-z0-9]+(?:[.:-][a-z0-9]+)*$/i.test(value) ? value : null;
}

function sameEquationContext(left: DocumentNode, right: DocumentNode): boolean {
  return left.page === right.page
    && left.parent_id === right.parent_id
    && left.provenance?.method === right.provenance?.method;
}

function mergeEquationNumber(
  equation: DocumentNode,
  numberNode: DocumentNode,
  number: string,
): DocumentNode {
  return {
    ...equation,
    number: equation.number || number,
    asset_ids: Array.from(new Set([...equation.asset_ids, ...numberNode.asset_ids])),
    representations: [...equation.representations, ...numberNode.representations],
  };
}

function coalesceEquationNumberNodes(nodes: DocumentNode[]): DocumentNode[] {
  const result: DocumentNode[] = [];
  for (let index = 0; index < nodes.length; index += 1) {
    const node = nodes[index];
    const number = node.kind === "equation" ? equationNumberText(node.text) : null;
    if (number) {
      const previous = result[result.length - 1];
      if (
        previous?.kind === "equation"
        && !equationNumberText(previous.text)
        && sameEquationContext(previous, node)
      ) {
        result[result.length - 1] = mergeEquationNumber(previous, node, number);
        continue;
      }
      const next = nodes[index + 1];
      if (
        next?.kind === "equation"
        && !equationNumberText(next.text)
        && sameEquationContext(node, next)
      ) {
        result.push(mergeEquationNumber(next, node, number));
        index += 1;
        continue;
      }
    }
    result.push(node);
  }
  return result;
}

function DocumentBundleView({
  text,
  artifacts,
  pageArtifacts,
  onSelectArtifact,
}: {
  text: string;
  artifacts: ArtifactEntry[];
  pageArtifacts?: PageArtifactIndex | null;
  onSelectArtifact: (relPath: string) => void;
}) {
  const [kindFilter, setKindFilter] = useState("structure");
  const [nodeLimit, setNodeLimit] = useState(INITIAL_BUNDLE_NODES);
  const bundle = useMemo(() => {
    try {
      return JSON.parse(text) as DocumentBundle;
    } catch {
      return null;
    }
  }, [text]);
  const displayNodes = useMemo(
    () => coalesceEquationNumberNodes(bundle?.nodes ?? []),
    [bundle],
  );
  const counts = useMemo(() => {
    const result = new Map<string, number>();
    for (const node of displayNodes) {
      result.set(node.kind, (result.get(node.kind) ?? 0) + 1);
    }
    return result;
  }, [displayNodes]);
  const filters = useMemo(
    () => ["structure", "all", ...Array.from(counts.keys()).sort()],
    [counts],
  );
  const matchingNodes = useMemo(
    () =>
      displayNodes.filter((node) =>
        kindFilter === "structure"
          ? STRUCTURAL_KINDS.has(node.kind)
          : kindFilter === "all" || node.kind === kindFilter,
      ),
    [displayNodes, kindFilter],
  );
  const visibleNodes = matchingNodes.slice(0, nodeLimit);
  const artifactsByPath = useMemo(
    () => new Map(artifacts.map((artifact) => [artifact.rel_path, artifact])),
    [artifacts],
  );
  const assetById = useMemo(
    () => new Map((bundle?.assets ?? []).map((asset) => [asset.id, asset])),
    [bundle],
  );
  const visualAssets = useMemo(
    () => (bundle?.assets ?? []).filter((asset) => asset.kind !== "page"),
    [bundle],
  );

  if (!bundle) {
    return <CodeView text={text} relPath="document_bundle.json" />;
  }

  const summary = [
    ["Pages", bundle.pages?.length ?? 0],
    ["Blocks", displayNodes.length],
    ["Figures", counts.get("figure") ?? 0],
    ["Tables", counts.get("table") ?? 0],
    ["Equations", counts.get("equation") ?? 0],
    ["Assets", bundle.assets?.length ?? 0],
  ] as const;

  return (
    <div className="p-5 space-y-5 text-sm text-gray-800 dark:text-gray-200">
      <section>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="text-lg font-semibold">DocumentBundle inspection</h2>
            <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
              Schema {bundle.schema_version} · {bundle.source_kind} · extracted with {bundle.extraction?.method}
            </p>
          </div>
          <code className="rounded bg-gray-100 dark:bg-gray-800 px-2 py-1 text-[11px]">
            {bundle.bundle_id}
          </code>
        </div>
        <div className="mt-4 grid grid-cols-2 sm:grid-cols-3 xl:grid-cols-6 gap-2">
          {summary.map(([label, value]) => (
            <div key={label} className="rounded-lg border border-gray-200 dark:border-gray-700 p-3">
              <div className="text-xl font-semibold">{value}</div>
              <div className="text-[11px] uppercase tracking-wide text-gray-500">{label}</div>
            </div>
          ))}
        </div>
      </section>

      <section className="grid gap-3 lg:grid-cols-2">
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 p-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Source provenance</h3>
          <div className="mt-2 space-y-2">
            {(bundle.origins ?? []).map((origin) => (
              <div key={origin.id}>
                <div className="font-medium">{origin.role} · {origin.kind}</div>
                <div className="mt-0.5 break-all font-mono text-[11px] text-gray-500">{origin.path}</div>
              </div>
            ))}
          </div>
        </div>
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 p-3">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Extraction quality</h3>
          {(bundle.quality ?? []).length === 0 ? (
            <p className="mt-2 text-xs text-emerald-700 dark:text-emerald-400">No extraction warnings recorded.</p>
          ) : (
            <ul className="mt-2 space-y-2">
              {bundle.quality.map((note, index) => (
                <li key={`${note.scope}-${index}`} className="text-xs">
                  <span className="font-medium">{note.severity} · {note.scope}:</span> {note.message}
                </li>
              ))}
            </ul>
          )}
        </div>
      </section>

      <section>
        <h3 className="text-sm font-semibold">Visual assets</h3>
        {visualAssets.length === 0 ? (
          <p className="mt-2 text-xs text-gray-500">
            {bundle.pages?.length
              ? `${bundle.pages.length} page renders are indexed in the compact Pages browser; no separate figures or media were extracted.`
              : "No visual assets were extracted."}
          </p>
        ) : (
          <div className="mt-2 grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            {visualAssets.map((asset) => {
              const indexedPage = indexedPageForPath(asset.rel_path, pageArtifacts);
              const manifestArtifact = artifactsByPath.get(asset.rel_path);
              const displayLabel = manifestArtifact?.label.trim() || asset.label;
              const canOpen = manifestArtifact !== undefined || indexedPage !== null;
              return (
                <button
                  key={asset.id}
                  type="button"
                  disabled={!canOpen}
                  onClick={() =>
                    onSelectArtifact(
                      indexedPage === null ? asset.rel_path : pageSelection(indexedPage),
                    )}
                  className="rounded-lg border border-gray-200 dark:border-gray-700 p-3 text-left
                             enabled:hover:border-gray-400 enabled:hover:bg-gray-50 dark:enabled:hover:bg-gray-800
                             disabled:opacity-60 transition-colors"
                >
                  <div className="flex justify-between gap-2">
                    <span className="font-medium truncate">{displayLabel}</span>
                    <span className="text-[10px] uppercase text-gray-500">{asset.kind}</span>
                  </div>
                  <div className="mt-1 text-[11px] text-gray-500">
                    {asset.page ? `Page ${asset.page} · ` : ""}
                    {asset.width && asset.height ? `${asset.width}×${asset.height} · ` : ""}
                    {asset.media_type}
                  </div>
                  <div className="mt-1 truncate font-mono text-[10px] text-gray-600 dark:text-gray-400">{asset.rel_path}</div>
                  {!canOpen && <div className="mt-1 text-[10px] text-amber-700 dark:text-amber-300">Asset missing from this report</div>}
                </button>
              );
            })}
          </div>
        )}
      </section>

      <section>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="text-sm font-semibold">Extracted structure</h3>
          <div className="flex flex-wrap gap-1" aria-label="Document node filters">
            {filters.map((kind) => (
              <button
                key={kind}
                type="button"
                onClick={() => {
                  setKindFilter(kind);
                  setNodeLimit(INITIAL_BUNDLE_NODES);
                }}
                className={`rounded-full border px-2 py-0.5 text-[11px] ${
                  kindFilter === kind
                    ? "border-gray-900 bg-gray-900 text-white dark:border-gray-100 dark:bg-gray-100 dark:text-gray-900"
                    : "border-gray-300 text-gray-600 dark:border-gray-700 dark:text-gray-300"
                }`}
              >
                {kind === "structure"
                  ? "Structural"
                  : kind === "all"
                    ? `All (${displayNodes.length})`
                    : `${kind} (${counts.get(kind) ?? 0})`}
              </button>
            ))}
          </div>
        </div>
        <p className="mt-1 text-xs text-gray-500">
          The readable document artifact contains the full extracted text. This view exposes semantic blocks and provenance.
        </p>
        <div className="mt-3 space-y-2">
          {visibleNodes.map((node) => (
            <details key={node.id} className="rounded-lg border border-gray-200 dark:border-gray-700">
              <summary className="cursor-pointer list-none p-3">
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <span className="font-medium">{node.label || node.kind}</span>
                    {node.number && <span className="ml-1 text-gray-500">({node.number})</span>}
                    <div className="mt-0.5 text-[11px] text-gray-500">
                      {node.id}{node.page ? ` · page ${node.page}` : ""} · {node.provenance?.method}
                    </div>
                  </div>
                  <span className="rounded bg-gray-100 dark:bg-gray-800 px-1.5 py-0.5 text-[10px] uppercase">
                    {node.kind}
                  </span>
                </div>
                {node.text && (
                  <p className="mt-2 line-clamp-3 whitespace-pre-wrap text-xs text-gray-600 dark:text-gray-300">
                    {node.text.length > 800 ? `${node.text.slice(0, 800)}…` : node.text}
                  </p>
                )}
              </summary>
              <div className="border-t border-gray-200 dark:border-gray-700 p-3 space-y-3">
                {node.text && (
                  <pre className="whitespace-pre-wrap text-xs font-sans leading-relaxed">{node.text}</pre>
                )}
                {node.asset_ids?.length > 0 && (
                  <div className="flex flex-wrap gap-1">
                    {node.asset_ids.map((assetId) => {
                      const asset = assetById.get(assetId);
                      const indexedPage = asset
                        ? indexedPageForPath(asset.rel_path, pageArtifacts)
                        : null;
                      return (
                        <button
                          key={assetId}
                          type="button"
                          disabled={
                            !asset
                            || (!artifactsByPath.has(asset.rel_path) && indexedPage === null)
                          }
                          onClick={() =>
                            asset
                            && onSelectArtifact(
                              indexedPage === null
                                ? asset.rel_path
                                : pageSelection(indexedPage),
                            )}
                          className="rounded border border-gray-300 dark:border-gray-600 px-2 py-1 text-[11px] disabled:opacity-50"
                        >
                          View {asset?.label ?? assetId}
                        </button>
                      );
                    })}
                  </div>
                )}
                {node.representations?.map((representation, index) => (
                  <details key={`${representation.format}-${index}`}>
                    <summary className="cursor-pointer text-xs font-medium text-gray-500">
                      {representation.format} representation
                    </summary>
                    <pre className="mt-1 overflow-auto rounded bg-gray-50 dark:bg-gray-900 p-2 text-[11px] whitespace-pre-wrap">
                      {formatRepresentation(representation.content)}
                    </pre>
                  </details>
                ))}
              </div>
            </details>
          ))}
          {visibleNodes.length === 0 && (
            <p className="rounded border border-dashed border-gray-300 dark:border-gray-700 p-4 text-xs text-gray-500">
              No nodes match this filter.
            </p>
          )}
          {visibleNodes.length < matchingNodes.length && (
            <button
              type="button"
              onClick={() => setNodeLimit((limit) => limit + INITIAL_BUNDLE_NODES)}
              className="w-full rounded-lg border border-gray-200 px-3 py-2 text-xs text-gray-500
                         hover:border-gray-400 hover:text-gray-700 dark:border-gray-700 dark:hover:border-gray-500
                         dark:hover:text-gray-200"
            >
              Show {Math.min(INITIAL_BUNDLE_NODES, matchingNodes.length - visibleNodes.length)} more
              {" "}({matchingNodes.length - visibleNodes.length} remaining)
            </button>
          )}
        </div>
      </section>
    </div>
  );
}

const Viewer = memo(function Viewer({
  runId,
  entry,
  content,
  artifacts,
  pageArtifacts,
  onSelectArtifact,
}: {
  runId: string;
  entry: ArtifactEntry | null;
  content: ArtifactContent;
  artifacts: ArtifactEntry[];
  pageArtifacts?: PageArtifactIndex | null;
  onSelectArtifact: (relPath: string) => void;
}) {
  if (content.kind === "image") {
    if (!content.base64) return <BinaryCard entry={entry} content={content} />;
    const mime = IMAGE_MIME[ext(entry?.rel_path ?? "")] ?? "image/png";
    return (
      <div className="p-4 flex justify-center">
        <img
          src={`data:${mime};base64,${content.base64}`}
          alt={entry?.label ?? "artifact"}
          className="max-w-full h-auto"
        />
      </div>
    );
  }
  if (content.kind === "pdf") {
    return <PdfView key={`${runId}:${entry?.rel_path}`} runId={runId} entry={entry} content={content} />;
  }
  if (content.kind === "binary" || content.text === null) {
    return <BinaryCard entry={entry} content={content} />;
  }

  const truncBanner = content.truncated && (
    <div className="mb-3 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
      Large file — showing the first {formatBytes(1_000_000)} of {formatBytes(content.bytes)}.
    </div>
  );

  switch (content.kind) {
    case "markdown":
      return (
        <div className="h-full flex flex-col">
          {truncBanner && <div className="px-4 pt-3">{truncBanner}</div>}
          <div className="flex-1 min-h-0 overflow-auto">
            <ReportViewer markdown={content.text} />
          </div>
        </div>
      );
    case "json": {
      if (entry?.rel_path === "context/document_bundle.json" && !content.truncated) {
        return (
          <DocumentBundleView
            text={content.text}
            artifacts={artifacts}
            pageArtifacts={pageArtifacts}
            onSelectArtifact={onSelectArtifact}
          />
        );
      }
      let pretty = content.text;
      if (!content.truncated) {
        try {
          pretty = JSON.stringify(JSON.parse(content.text), null, 2);
        } catch {
          // leave as-is
        }
      }
      return (
        <div className="p-4">
          {truncBanner}
          <CodeView text={pretty} relPath="artifact.json" />
        </div>
      );
    }
    case "csv":
      return (
        <div className="p-4">
          {truncBanner}
          <CsvView text={content.text} relPath={entry?.rel_path ?? ""} />
        </div>
      );
    case "code":
      return (
        <div className="p-4">
          {truncBanner}
          <CodeView text={content.text} relPath={entry?.rel_path ?? ""} />
        </div>
      );
    default:
      return (
        <div className="p-4">
          {truncBanner}
          <pre className="text-xs font-mono leading-relaxed whitespace-pre-wrap text-gray-800 dark:text-gray-200">
            {content.text}
          </pre>
        </div>
      );
  }
});

const GROUPS: { id: string; label: string; description?: string }[] = [
  { id: "report", label: "Report" },
  { id: "document", label: "Document" },
  { id: "pages", label: "Pages" },
  { id: "figures", label: "Figures" },
  { id: "tables", label: "Tables" },
  { id: "equations", label: "Equations" },
  { id: "context", label: "Context" },
  {
    id: "agent_response",
    label: "Agent reports",
    description: "Provider responses, including retries and merge calls.",
  },
  {
    id: "step",
    label: "Step outputs",
    description: "One finalized result per workflow step, after agent merging.",
  },
  { id: "files", label: "Files" },
];

function pageNumber(entry: ArtifactEntry) {
  const match = `${entry.label} ${entry.rel_path}`.match(/(?:page[-_\s]*)(\d+)/i);
  return match ? Number(match[1]) : Number.MAX_SAFE_INTEGER;
}

const PageBrowser = memo(function PageBrowser({
  items,
  index,
  onSelect,
  selected,
}: {
  items: ArtifactEntry[];
  index?: PageArtifactIndex | null;
  onSelect: (path: string) => void;
  selected: string;
}) {
  const sorted = useMemo(
    () => [...items].sort((a, b) => pageNumber(a) - pageNumber(b)),
    [items],
  );
  const indexedCount = index && Number.isSafeInteger(index.count)
    ? Math.min(Math.max(index.count, 0), MAX_INDEXED_PAGES)
    : 0;
  const legacyPages = useMemo(
    () => sorted
      .map((item) => ({ number: pageNumber(item), item }))
      .filter(({ number }) => Number.isFinite(number) && number !== Number.MAX_SAFE_INTEGER),
    [sorted],
  );
  const selectedNumber = useMemo(() => {
    if (index) {
      const page = selectedPage(selected);
      return page !== null && page <= indexedCount ? page : null;
    }
    return legacyPages.find(({ item }) => item.rel_path === selected)?.number ?? null;
  }, [index, indexedCount, legacyPages, selected]);
  const [draftPage, setDraftPage] = useState("");

  useEffect(() => {
    setDraftPage(selectedNumber?.toString() ?? "");
  }, [selectedNumber]);

  const lastPage = index
    ? indexedCount
    : (legacyPages[legacyPages.length - 1]?.number ?? 0);
  const selectedLegacyIndex = index
    ? -1
    : legacyPages.findIndex(({ number }) => number === selectedNumber);
  const hasPrevious = selectedNumber !== null && (index
    ? selectedNumber > 1
    : selectedLegacyIndex > 0);
  const hasNext = lastPage > 0 && (selectedNumber === null || (index
    ? selectedNumber < indexedCount
    : selectedLegacyIndex < legacyPages.length - 1));

  const selectPage = (page: number) => {
    if (index) {
      if (Number.isInteger(page) && page >= 1 && page <= indexedCount) {
        onSelect(pageSelection(page));
        return true;
      }
      return false;
    }
    const match = legacyPages.find(({ number }) => number === page);
    if (!match) return false;
    onSelect(match.item.rel_path);
    return true;
  };

  const commitDraft = () => {
    const page = Number(draftPage);
    if (!Number.isInteger(page) || !selectPage(page)) {
      setDraftPage(selectedNumber?.toString() ?? "");
    }
  };

  const move = (direction: -1 | 1) => {
    if (selectedNumber === null) {
      if (direction === 1) {
        selectPage(index ? 1 : (legacyPages[0]?.number ?? 0));
      }
      return;
    }
    if (index) {
      selectPage(selectedNumber + direction);
      return;
    }
    const target = legacyPages[selectedLegacyIndex + direction];
    if (target) selectPage(target.number);
  };

  return (
    <div
      className="flex items-center overflow-hidden rounded-md border border-gray-200 bg-white
                 dark:border-gray-700 dark:bg-gray-900"
    >
      <button
        type="button"
        onClick={() => move(-1)}
        disabled={!hasPrevious}
        aria-label="Previous page"
        title="Previous page"
        className="shrink-0 border-r border-gray-200 px-1.5 py-1.5 text-gray-500 transition-colors
                   enabled:hover:bg-gray-100 enabled:hover:text-gray-800 disabled:opacity-30
                   dark:border-gray-700 dark:text-gray-400 dark:enabled:hover:bg-gray-800 dark:enabled:hover:text-gray-100"
      >
        <svg aria-hidden="true" className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="m15 18-6-6 6-6" />
        </svg>
      </button>
      <form
        className="flex min-w-0 flex-1 items-center"
        onSubmit={(event) => {
          event.preventDefault();
          commitDraft();
        }}
      >
        <input
          type="text"
          inputMode="numeric"
          pattern="[0-9]*"
          value={draftPage}
          onChange={(event) => setDraftPage(event.target.value)}
          onBlur={commitDraft}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              setDraftPage(selectedNumber?.toString() ?? "");
              event.currentTarget.blur();
            }
          }}
          aria-label="Page number"
          placeholder="Page"
          title={lastPage > 0 ? `Enter a page from 1 to ${lastPage}` : "No pages available"}
          className="min-w-0 flex-1 bg-transparent px-2 py-1 text-right text-xs tabular-nums text-gray-700
                     outline-none placeholder:text-gray-400 dark:text-gray-200 dark:placeholder:text-gray-600"
        />
        <span className="shrink-0 pr-2 text-[11px] tabular-nums text-gray-400 dark:text-gray-500">
          / {lastPage}
        </span>
      </form>
      <button
        type="button"
        onClick={() => move(1)}
        disabled={!hasNext}
        aria-label="Next page"
        title="Next page"
        className="shrink-0 border-l border-gray-200 px-1.5 py-1.5 text-gray-500 transition-colors
                   enabled:hover:bg-gray-100 enabled:hover:text-gray-800 disabled:opacity-30
                   dark:border-gray-700 dark:text-gray-400 dark:enabled:hover:bg-gray-800 dark:enabled:hover:text-gray-100"
      >
        <svg aria-hidden="true" className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="m9 18 6-6-6-6" />
        </svg>
      </button>
    </div>
  );
});

const ArtifactList = memo(function ArtifactList({
  items,
  onSelect,
  selected,
}: {
  items: ArtifactEntry[];
  onSelect: (path: string) => void;
  selected: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const visible = expanded ? items : items.slice(0, 10);

  return (
    <>
      <ul className="space-y-0.5">
        {visible.map((item) => (
          <li key={item.rel_path}>
            <button
              onClick={() => onSelect(item.rel_path)}
              title={`${item.rel_path} · ${formatBytes(item.bytes)}`}
              className={`w-full truncate rounded px-2 py-1 text-left text-xs transition-colors ${
                selected === item.rel_path
                  ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                  : "text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
              }`}
            >
              {item.label}
            </button>
          </li>
        ))}
      </ul>
      {items.length > 10 && (
        <button
          type="button"
          onClick={() => setExpanded((value) => !value)}
          className="mt-1 w-full rounded px-2 py-1 text-left text-[11px] text-gray-600 hover:bg-gray-100
                     hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
        >
          {expanded ? "Show fewer" : `Show ${items.length - 10} more`}
        </button>
      )}
    </>
  );
});

interface AgentReportGroup {
  key: string;
  label: string;
  order: number;
  items: ArtifactEntry[];
}

function artifactStem(entry: ArtifactEntry): string {
  return entry.rel_path
    .split("/")
    .pop()
    ?.replace(/\.[^.]+$/, "") ?? "";
}

function comparableProducer(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function stepOutputIdentity(entry: ArtifactEntry, fallbackOrder: number) {
  const stem = artifactStem(entry);
  const match = stem.match(/^(\d+)_([^]*)$/);
  const order = match ? Number(match[1]) : fallbackOrder;
  const key = comparableProducer(match?.[2] ?? stem);
  // Multi-agent outputs used to retain a synthetic "[Agent]" suffix even
  // after the reports had been merged. It identifies a provider call, not a
  // separate workflow step, so omit it from the group heading.
  const label = entry.label.replace(/\s+\[[^\]]+\]\s*$/, "").trim() || entry.label;
  return { key, label, order };
}

function agentResponseProducer(entry: ArtifactEntry): { key: string; merge: boolean } {
  const stem = artifactStem(entry);
  const producer = stem.split(/--[a-f0-9]{12}--attempt-/i, 1)[0] ?? stem;
  const merge = producer.startsWith("merge-");
  return {
    key: comparableProducer(merge ? producer.slice("merge-".length) : producer),
    merge,
  };
}

/**
 * Agent-response filenames contain a producer-derived key. Finalized
 * step-output filenames contain the corresponding key and a leading output
 * number, so they provide a durable step order without loading report.json
 * (Sources remains manifest-only until an item is read).
 */
function groupAgentReports(
  items: ArtifactEntry[],
  stepOutputs: ArtifactEntry[],
): AgentReportGroup[] {
  const steps = stepOutputs.map(stepOutputIdentity);
  const groups = new Map<string, AgentReportGroup>();

  items.forEach((item, itemIndex) => {
    const producer = agentResponseProducer(item);
    // Prefer the longest match when one step id prefixes another (for example,
    // "technical" and "technical-appendix"). A suffix denotes an agent or a
    // fan-out unit belonging to that logical step.
    const step = steps
      .filter(({ key }) => producer.key === key || producer.key.startsWith(`${key}-`))
      .sort((a, b) => b.key.length - a.key.length)[0];
    const key = step?.key ?? `unmatched:${producer.key}`;
    const existing = groups.get(key);
    if (existing) {
      existing.items.push(item);
      return;
    }
    groups.set(key, {
      key,
      label: step?.label ?? item.label.split(" · Attempt", 1)[0] ?? item.label,
      order: step?.order ?? Number.MAX_SAFE_INTEGER - items.length + itemIndex,
      items: [item],
    });
  });

  return [...groups.values()]
    .sort((a, b) => a.order - b.order || a.label.localeCompare(b.label))
    .map((group) => ({
      ...group,
      items: [...group.items].sort((a, b) => {
        const producerA = agentResponseProducer(a);
        const producerB = agentResponseProducer(b);
        // Individual agent responses precede the cross-agent merge report.
        if (producerA.merge !== producerB.merge) return producerA.merge ? 1 : -1;
        return a.rel_path.localeCompare(b.rel_path, undefined, { numeric: true });
      }),
    }));
}

const AgentReportList = memo(function AgentReportList({
  items,
  stepOutputs,
  onSelect,
  selected,
}: {
  items: ArtifactEntry[];
  stepOutputs: ArtifactEntry[];
  onSelect: (path: string) => void;
  selected: string;
}) {
  const groups = useMemo(
    () => groupAgentReports(items, stepOutputs),
    [items, stepOutputs],
  );

  return (
    <div className="space-y-2.5">
      {groups.map((group) => (
        <section key={group.key}>
          <h5
            className="mb-1 truncate px-2 text-[11px] font-medium text-gray-700 dark:text-gray-300"
            title={group.label}
          >
            {group.label}
          </h5>
          <ArtifactList
            items={group.items}
            selected={selected}
            onSelect={onSelect}
          />
        </section>
      ))}
    </div>
  );
});

export default function ArtifactExplorer({
  runId,
  fallbackMarkdown,
  deferInitialArtifact = false,
  preload = null,
  selectionRequest = null,
}: Props) {
  const [manifest, setManifest] = useState<RunManifest | null>(null);
  const [manifestError, setManifestError] = useState<string | null>(null);
  const [manifestAttempt, setManifestAttempt] = useState(0);
  const [selected, setSelected] = useState<string>(
    deferInitialArtifact ? "" : "report.md",
  );
  const [selectionHistory, setSelectionHistory] = useState<string[]>([]);
  const [content, setContent] = useState<ArtifactContent | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [navigationOpen, setNavigationOpen] = useState(true);
  const [treeWidth, setTreeWidth] = usePersistentPanelWidth(
    "pipeline.ui.artifactTreeWidth",
    224,
    176,
    360,
  );

  useEffect(() => {
    let live = true;
    // Clear every run-scoped value before requesting the next manifest. This
    // also prevents the content effect from combining a new run ID with the
    // previous run's selection.
    setManifest(null);
    setManifestError(null);
    setSelected(deferInitialArtifact ? "" : "report.md");
    setSelectionHistory([]);
    setContent(null);
    setLoadError(null);
    const request = preload?.runId === runId
      ? preload.manifest.catch(() =>
          invoke<RunManifest>("get_run_manifest", { runId }),
        )
      : invoke<RunManifest>("get_run_manifest", { runId });
    request
      .then((m) => {
        if (live) {
          const initial = deferInitialArtifact
            ? ""
            : m.artifacts.some((artifact) => artifact.rel_path === "report.md")
              ? "report.md"
              : (m.artifacts[0]?.rel_path ?? "");
          setSelected(initial);
          setManifest(m);
        }
      })
      .catch((e) => {
        console.error("Failed to load run manifest:", e);
        if (live) setManifestError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      live = false;
    };
  }, [deferInitialArtifact, preload, runId, manifestAttempt]);

  useEffect(() => {
    if (!manifest || !selected) return;
    let live = true;
    setContent(null);
    setLoadError(null);
    const compactPage = manifest.page_artifacts ? selectedPage(selected) : null;
    const preloadedReadable = preload?.runId === runId
      && selected === "context/document.md"
      ? preload.readableDocument
      : null;
    const request = preloadedReadable
      ? preloadedReadable.catch(() =>
          invoke<ArtifactContent>("read_artifact", { runId, relPath: selected }),
        )
      : compactPage === null
        ? invoke<ArtifactContent>("read_artifact", { runId, relPath: selected })
        : invoke<ArtifactContent>("read_page_artifact", { runId, page: compactPage });
    request
      .then((c) => {
        if (live) setContent(c);
      })
      .catch((e) => {
        console.error("Failed to read artifact:", e);
        if (live) {
          setContent(null);
          setLoadError(e instanceof Error ? e.message : String(e));
        }
      });
    return () => {
      live = false;
    };
  }, [manifest, preload, runId, selected]);

  const artifactsByGroup = useMemo(() => {
    const grouped = new Map<string, ArtifactEntry[]>();
    for (const artifact of manifest?.artifacts ?? []) {
      const items = grouped.get(artifact.group);
      if (items) {
        items.push(artifact);
      } else {
        grouped.set(artifact.group, [artifact]);
      }
    }
    return grouped;
  }, [manifest]);
  const artifactsByPath = useMemo(
    () => new Map((manifest?.artifacts ?? []).map((artifact) => [artifact.rel_path, artifact])),
    [manifest],
  );
  const selectArtifact = useCallback((next: string) => {
    if (!next || next === selected) return;
    if (selected) {
      setSelectionHistory((history) => [...history.slice(-99), selected]);
    }
    setSelected(next);
  }, [selected]);
  useEffect(() => {
    if (!manifest || !selectionRequest) return;
    let next = "";
    if (selectionRequest.relPath) {
      const indexedPage = indexedPageForPath(selectionRequest.relPath, manifest.page_artifacts);
      if (indexedPage !== null) {
        next = pageSelection(indexedPage);
      } else if (manifest.artifacts.some((artifact) => artifact.rel_path === selectionRequest.relPath)) {
        next = selectionRequest.relPath;
      }
    }
    if (!next && selectionRequest.page && selectionRequest.page > 0) {
      if (manifest.page_artifacts && selectionRequest.page <= manifest.page_artifacts.count) {
        next = pageSelection(selectionRequest.page);
      } else {
        next = manifest.artifacts.find((artifact) =>
          artifact.group === "pages" && pageNumber(artifact) === selectionRequest.page,
        )?.rel_path ?? "";
      }
    }
    if (next) {
      setNavigationOpen(true);
      selectArtifact(next);
    }
  }, [manifest, selectArtifact, selectionRequest]);
  const goBack = useCallback(() => {
    const previous = selectionHistory[selectionHistory.length - 1];
    if (!previous) return;
    setSelectionHistory((history) => history.slice(0, -1));
    setSelected(previous);
  }, [selectionHistory]);

  // Graceful degradation: no manifest → plain report view.
  if (manifestError) {
    return (
      <div className="h-full overflow-y-auto">
        <div role="alert" className="m-4 rounded-lg border border-red-200 dark:border-red-900 bg-red-50 dark:bg-red-950/30 p-3 text-sm text-red-700 dark:text-red-300">
          <p className="font-medium">Could not load this run's artifact manifest.</p>
          <p className="mt-1 text-xs">{manifestError}</p>
          <button
            type="button"
            onClick={() => setManifestAttempt((attempt) => attempt + 1)}
            className="mt-2 rounded border border-red-300 dark:border-red-800 px-2 py-1 text-xs hover:bg-red-100 dark:hover:bg-red-900/40"
          >
            Retry
          </button>
        </div>
        {fallbackMarkdown && (
          <section aria-label="Fallback report">
            <p className="mx-4 text-xs text-gray-500">Showing the fallback report.</p>
            <ReportViewer markdown={fallbackMarkdown} />
          </section>
        )}
      </div>
    );
  }
  if (!manifest) {
    return (
      <div className="flex items-center justify-center h-full text-gray-400">
        <div className="animate-spin w-5 h-5 border-2 border-gray-300 border-t-gray-600 rounded-full" />
      </div>
    );
  }

  const pageIndex = manifest.page_artifacts ?? null;
  const compactPage = pageIndex ? selectedPage(selected) : null;
  const averagePageBytes = pageIndex && pageIndex.count > 0
    ? Math.round(pageIndex.total_bytes / pageIndex.count)
    : 0;
  const entry = compactPage === null
    ? (artifactsByPath.get(selected) ?? null)
    : {
        rel_path: `artifacts/pages/page-${compactPage}.${pageIndex?.extension ?? "jpg"}`,
        label: `Page ${compactPage}`,
        kind: "image",
        bytes: averagePageBytes,
        sha256: "",
        group: "pages",
      };

  return (
    <div className="flex h-full min-h-0">
      {/* Tree */}
      {navigationOpen ? (
        <nav
          style={{ width: treeWidth }}
          className="relative shrink-0 border-r border-gray-200 dark:border-gray-800"
        >
          <div className="h-full space-y-4 overflow-y-auto p-3">
            <div className="flex items-center justify-between gap-2 px-1">
              <span className="text-[11px] font-semibold uppercase tracking-[0.12em] text-gray-600 dark:text-gray-400">
                Artifacts
              </span>
              <button
                type="button"
                onClick={() => setNavigationOpen(false)}
                aria-label="Hide artifact browser"
                className="rounded p-1 text-gray-500 hover:bg-gray-100 hover:text-gray-800
                           focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400
                           dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
              >
                <svg aria-hidden="true" className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.8}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="m9 6 6 6-6 6" />
                </svg>
              </button>
            </div>
            {GROUPS.map((g) => {
              const items = artifactsByGroup.get(g.id) ?? [];
              const compactPages = g.id === "pages" ? manifest.page_artifacts : null;
              if (items.length === 0 && !compactPages?.count) return null;
              return (
                <div key={g.id}>
                  <h4 className="mb-1.5 text-[10px] font-semibold uppercase tracking-widest text-gray-600 dark:text-gray-400">
                    {g.label}
                  </h4>
                  {g.description && (
                    <p className="mb-1.5 px-0.5 text-[10px] leading-snug text-gray-500 dark:text-gray-500">
                      {g.description}
                    </p>
                  )}
                  {g.id === "pages" ? (
                    <PageBrowser
                      items={items}
                      index={compactPages}
                      selected={selected}
                      onSelect={selectArtifact}
                    />
                  ) : g.id === "agent_response" ? (
                    <AgentReportList
                      items={items}
                      stepOutputs={artifactsByGroup.get("step") ?? []}
                      selected={selected}
                      onSelect={selectArtifact}
                    />
                  ) : (
                    <ArtifactList
                      items={items}
                      selected={selected}
                      onSelect={selectArtifact}
                    />
                  )}
                </div>
              );
            })}
          </div>
          <ResizeHandle
            currentWidth={treeWidth}
            defaultWidth={224}
            label="Resize artifact browser"
            min={176}
            max={360}
            onResize={setTreeWidth}
          />
        </nav>
      ) : (
        <button
          type="button"
          onClick={() => setNavigationOpen(true)}
          aria-label="Show artifact browser"
          title="Show artifacts"
          className="flex w-10 shrink-0 items-start justify-center border-r border-gray-200 pt-4 text-gray-500
                     hover:bg-gray-50 hover:text-gray-800 focus-visible:outline-none focus-visible:ring-2
                     focus-visible:ring-inset focus-visible:ring-gray-400 dark:border-gray-800 dark:hover:bg-gray-900
                     dark:text-gray-400 dark:hover:text-gray-100"
        >
          <svg aria-hidden="true" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.7}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M7 4.5h10M7 9.5h10M7 14.5h6M4 4.5h.01M4 9.5h.01M4 14.5h.01" />
          </svg>
        </button>
      )}

      {/* Viewer */}
      <div className="flex min-w-0 flex-1 flex-col">
        {content && (content.kind === "image" || content.kind === "pdf") && (
          <div className="flex shrink-0 items-center gap-3 border-b border-gray-200 bg-white px-4 py-2
                          dark:border-gray-800 dark:bg-gray-950">
            <button
              type="button"
              onClick={goBack}
              disabled={selectionHistory.length === 0}
              className="inline-flex items-center gap-1.5 rounded px-2 py-1 text-xs font-medium text-gray-600
                         hover:bg-gray-100 hover:text-gray-900 disabled:cursor-default disabled:opacity-40
                         dark:text-gray-300 dark:hover:bg-gray-800 dark:hover:text-white"
            >
              <svg aria-hidden="true" className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.8}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m15 18-6-6 6-6" />
              </svg>
              Back
            </button>
            <span className="min-w-0 truncate text-xs text-gray-500 dark:text-gray-400">
              {entry?.label ?? "Figure"}
            </span>
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-auto">
          {loadError ? (
            <div className="p-6 text-sm text-gray-500 dark:text-gray-400">
              Could not read this artifact: {loadError}
            </div>
          ) : !selected ? (
            <div className="flex h-full items-center justify-center p-8 text-center text-sm text-gray-500 dark:text-gray-400">
              Select an artifact to preview it.
            </div>
          ) : content ? (
            <Viewer
              runId={runId}
              entry={entry}
              content={content}
              artifacts={manifest.artifacts}
              pageArtifacts={manifest.page_artifacts}
              onSelectArtifact={selectArtifact}
            />
          ) : (
            <div className="flex items-center justify-center h-full text-gray-400">
              <div className="animate-spin w-5 h-5 border-2 border-gray-300 border-t-gray-600 rounded-full" />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
