// Run-artifact explorer: a manifest-driven tree beside a kind-dispatched
// viewer. All file bytes arrive through the `read_artifact` Tauri command
// (validated + size-capped in Rust) — never file:// URLs — so rendering is
// identical across WKWebView / WebView2 / WebKitGTK.

import { memo, useEffect, useMemo, useState } from "react";
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

interface RunManifest {
  run_id: string;
  created: string;
  input_path: string;
  input_mode: string;
  profile_name: string;
  provider: string;
  artifacts: ArtifactEntry[];
  page_artifacts?: PageArtifactIndex | null;
}

interface ArtifactContent {
  kind: string;
  bytes: number;
  text: string | null;
  base64: string | null;
  truncated: boolean;
  abs_path: string;
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

function formatRepresentation(value: unknown): string {
  const text = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  if (!text) return "";
  return text.length > 4_000 ? `${text.slice(0, 4_000)}\n…` : text;
}

const STRUCTURAL_KINDS = new Set([
  "section", "equation", "table", "figure", "theorem", "proposition", "lemma", "corollary",
]);
const INITIAL_BUNDLE_NODES = 100;

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
  const counts = useMemo(() => {
    const result = new Map<string, number>();
    for (const node of bundle?.nodes ?? []) {
      result.set(node.kind, (result.get(node.kind) ?? 0) + 1);
    }
    return result;
  }, [bundle]);
  const filters = useMemo(
    () => ["structure", "all", ...Array.from(counts.keys()).sort()],
    [counts],
  );
  const matchingNodes = useMemo(
    () =>
      (bundle?.nodes ?? []).filter((node) =>
        kindFilter === "structure"
          ? STRUCTURAL_KINDS.has(node.kind)
          : kindFilter === "all" || node.kind === kindFilter,
      ),
    [bundle, kindFilter],
  );
  const visibleNodes = matchingNodes.slice(0, nodeLimit);
  const artifactPaths = useMemo(
    () => new Set(artifacts.map((artifact) => artifact.rel_path)),
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
    ["Blocks", bundle.nodes?.length ?? 0],
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
              const canOpen = artifactPaths.has(asset.rel_path) || indexedPage !== null;
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
                    <span className="font-medium truncate">{asset.label}</span>
                    <span className="text-[10px] uppercase text-gray-500">{asset.kind}</span>
                  </div>
                  <div className="mt-1 text-[11px] text-gray-500">
                    {asset.page ? `Page ${asset.page} · ` : ""}
                    {asset.width && asset.height ? `${asset.width}×${asset.height} · ` : ""}
                    {asset.media_type}
                  </div>
                  <div className="mt-1 truncate font-mono text-[10px] text-gray-600 dark:text-gray-400">{asset.rel_path}</div>
                  {!canOpen && <div className="mt-1 text-[10px] text-amber-700 dark:text-amber-300">Asset missing from this run</div>}
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
                    ? `All (${bundle.nodes?.length ?? 0})`
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
                            || (!artifactPaths.has(asset.rel_path) && indexedPage === null)
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
  entry,
  content,
  artifacts,
  pageArtifacts,
  onSelectArtifact,
}: {
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

const GROUPS: { id: string; label: string }[] = [
  { id: "report", label: "Report" },
  { id: "document", label: "Document" },
  { id: "pages", label: "Pages" },
  { id: "figures", label: "Figures" },
  { id: "tables", label: "Tables" },
  { id: "equations", label: "Equations" },
  { id: "context", label: "Context" },
  { id: "step", label: "Steps" },
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
  const [query, setQuery] = useState("");
  const [rangeStart, setRangeStart] = useState(0);
  const sorted = useMemo(
    () => [...items].sort((a, b) => pageNumber(a) - pageNumber(b)),
    [items],
  );
  const rangeSize = 25;
  const indexedCount = index && Number.isSafeInteger(index.count)
    ? Math.min(Math.max(index.count, 0), MAX_INDEXED_PAGES)
    : 0;
  const pageCount = index ? indexedCount : sorted.length;
  const ranges = Array.from(
    { length: Math.ceil(pageCount / rangeSize) },
    (_, index) => index * rangeSize,
  );
  const needle = query.trim().toLowerCase();
  const visible = useMemo(() => {
    if (!index) {
      return (needle
        ? sorted.filter((item) =>
            `${item.label} ${pageNumber(item)}`.toLowerCase().includes(needle),
          )
        : sorted.slice(rangeStart, rangeStart + rangeSize)
      ).map((item) => ({ number: pageNumber(item), item }));
    }
    const start = needle ? 1 : rangeStart + 1;
    const end = needle
      ? indexedCount
      : Math.min(rangeStart + rangeSize, indexedCount);
    const pages: Array<{ number: number; item: null }> = [];
    for (let page = start; page <= end; page += 1) {
      if (!needle || `page ${page} ${page}`.includes(needle)) {
        pages.push({ number: page, item: null });
      }
    }
    return pages;
  }, [index, indexedCount, needle, rangeStart, sorted]);

  return (
    <div>
      <div className="mb-2 flex gap-1.5">
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          aria-label="Search pages"
          placeholder="Find page"
          className="min-w-0 flex-1 rounded-md border border-gray-200 bg-white px-2 py-1 text-xs text-gray-700
                     outline-none focus:border-gray-400 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200"
        />
        {ranges.length > 1 && !needle && (
          <select
            aria-label="Page range"
            value={rangeStart}
            onChange={(event) => setRangeStart(Number(event.target.value))}
            className="w-[5.5rem] rounded-md border border-gray-200 bg-white px-1 py-1 text-[11px] text-gray-500
                       outline-none focus:border-gray-400 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-400"
          >
            {ranges.map((start) => (
              <option key={start} value={start}>
                {start + 1}–{Math.min(start + rangeSize, pageCount)}
              </option>
            ))}
          </select>
        )}
      </div>
      <div className="grid grid-cols-4 gap-1">
        {visible.map(({ number, item }) => {
          const selection = index ? pageSelection(number) : item!.rel_path;
          const approximateBytes = index && index.count > 0
            ? index.total_bytes / index.count
            : item?.bytes;
          return (
            <button
              key={selection}
              type="button"
              onClick={() => onSelect(selection)}
              title={`Page ${number}${
                approximateBytes ? ` · about ${formatBytes(approximateBytes)}` : ""
              }`}
              aria-label={`Page ${number}`}
              className={`rounded-md px-1 py-1.5 text-center text-[11px] tabular-nums transition-colors ${
                selected === selection
                  ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                  : "bg-gray-100/70 text-gray-600 hover:bg-gray-200 dark:bg-gray-800/60 dark:text-gray-300 dark:hover:bg-gray-800"
              }`}
            >
              {Number.isFinite(number) && number !== Number.MAX_SAFE_INTEGER
                ? number
                : item?.label}
            </button>
          );
        })}
      </div>
      {visible.length === 0 && (
        <p className="px-1 py-2 text-xs text-gray-500 dark:text-gray-400">No matching pages.</p>
      )}
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

export default function ArtifactExplorer({
  runId,
  fallbackMarkdown,
  deferInitialArtifact = false,
}: Props) {
  const [manifest, setManifest] = useState<RunManifest | null>(null);
  const [manifestError, setManifestError] = useState<string | null>(null);
  const [manifestAttempt, setManifestAttempt] = useState(0);
  const [selected, setSelected] = useState<string>(
    deferInitialArtifact ? "" : "report.md",
  );
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
    setContent(null);
    setLoadError(null);
    invoke<RunManifest>("get_run_manifest", { runId })
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
  }, [deferInitialArtifact, runId, manifestAttempt]);

  useEffect(() => {
    if (!manifest || !selected) return;
    let live = true;
    setContent(null);
    setLoadError(null);
    const compactPage = manifest.page_artifacts ? selectedPage(selected) : null;
    const request = compactPage === null
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
  }, [manifest, runId, selected]);

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
                  {g.id === "pages" ? (
                    <PageBrowser
                      items={items}
                      index={compactPages}
                      selected={selected}
                      onSelect={setSelected}
                    />
                  ) : (
                    <ArtifactList
                      items={items}
                      selected={selected}
                      onSelect={setSelected}
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
      <div className="flex-1 min-w-0 overflow-auto">
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
            entry={entry}
            content={content}
            artifacts={manifest.artifacts}
            pageArtifacts={manifest.page_artifacts}
            onSelectArtifact={setSelected}
          />
        ) : (
          <div className="flex items-center justify-center h-full text-gray-400">
            <div className="animate-spin w-5 h-5 border-2 border-gray-300 border-t-gray-600 rounded-full" />
          </div>
        )}
      </div>
    </div>
  );
}
