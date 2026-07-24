// Run-artifact explorer: a manifest-driven tree beside a kind-dispatched
// viewer. All file bytes arrive through the `read_artifact` Tauri command
// (validated + size-capped in Rust) — never file:// URLs — so rendering is
// identical across WKWebView / WebView2 / WebKitGTK.

import { useEffect, useState } from "react";
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

interface RunManifest {
  run_id: string;
  created: string;
  input_path: string;
  input_mode: string;
  profile_name: string;
  provider: string;
  artifacts: ArtifactEntry[];
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
}

/** Skip syntax highlighting beyond this size — highlightAuto gets slow. */
const MAX_HIGHLIGHT_CHARS = 200_000;
const MAX_CSV_ROWS = 200;

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

function CodeView({ text, relPath }: { text: string; relPath: string }) {
  const lang = EXT_LANGUAGE[ext(relPath)];
  let html: string | null = null;
  if (text.length <= MAX_HIGHLIGHT_CHARS) {
    try {
      html = lang && hljs.getLanguage(lang)
        ? hljs.highlight(text, { language: lang }).value
        : hljs.highlightAuto(text).value;
    } catch {
      html = null;
    }
  }
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
        <p className="text-xs text-gray-400 mt-2">
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
  return (
    <div className="max-w-sm mx-auto mt-12 p-5 rounded-lg border border-gray-200 dark:border-gray-700 text-center">
      <p className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
        {entry?.label ?? "Binary file"}
      </p>
      <p className="text-xs text-gray-400 mb-4">
        {formatBytes(content.bytes)} — not previewable in the app.
      </p>
      <button
        onClick={() => openExternal(content.abs_path).catch(console.error)}
        className="py-1.5 px-4 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                   text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
      >
        Open in system viewer
      </button>
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

function DocumentBundleView({
  text,
  artifacts,
  onSelectArtifact,
}: {
  text: string;
  artifacts: ArtifactEntry[];
  onSelectArtifact: (relPath: string) => void;
}) {
  const [kindFilter, setKindFilter] = useState("structure");
  let bundle: DocumentBundle;
  try {
    bundle = JSON.parse(text) as DocumentBundle;
  } catch {
    return <CodeView text={text} relPath="document_bundle.json" />;
  }

  const counts = new Map<string, number>();
  for (const node of bundle.nodes ?? []) {
    counts.set(node.kind, (counts.get(node.kind) ?? 0) + 1);
  }
  const filters = ["structure", "all", ...Array.from(counts.keys()).sort()];
  const visibleNodes = (bundle.nodes ?? []).filter((node) =>
    kindFilter === "structure"
      ? STRUCTURAL_KINDS.has(node.kind)
      : kindFilter === "all" || node.kind === kindFilter,
  );
  const artifactPaths = new Set(artifacts.map((artifact) => artifact.rel_path));
  const assetById = new Map((bundle.assets ?? []).map((asset) => [asset.id, asset]));
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
        {(bundle.assets ?? []).length === 0 ? (
          <p className="mt-2 text-xs text-gray-500">No visual assets were extracted.</p>
        ) : (
          <div className="mt-2 grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            {bundle.assets.map((asset) => {
              const canOpen = artifactPaths.has(asset.rel_path);
              return (
                <button
                  key={asset.id}
                  type="button"
                  disabled={!canOpen}
                  onClick={() => onSelectArtifact(asset.rel_path)}
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
                  <div className="mt-1 truncate font-mono text-[10px] text-gray-400">{asset.rel_path}</div>
                  {!canOpen && <div className="mt-1 text-[10px] text-amber-600">Asset missing from this run</div>}
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
                onClick={() => setKindFilter(kind)}
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
                      return (
                        <button
                          key={assetId}
                          type="button"
                          disabled={!asset || !artifactPaths.has(asset.rel_path)}
                          onClick={() => asset && onSelectArtifact(asset.rel_path)}
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
        </div>
      </section>
    </div>
  );
}

function Viewer({
  entry,
  content,
  artifacts,
  onSelectArtifact,
}: {
  entry: ArtifactEntry | null;
  content: ArtifactContent;
  artifacts: ArtifactEntry[];
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
}

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

export default function ArtifactExplorer({ runId, fallbackMarkdown }: Props) {
  const [manifest, setManifest] = useState<RunManifest | null>(null);
  const [manifestError, setManifestError] = useState<string | null>(null);
  const [manifestAttempt, setManifestAttempt] = useState(0);
  const [selected, setSelected] = useState<string>("report.md");
  const [content, setContent] = useState<ArtifactContent | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
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
    setSelected("report.md");
    setContent(null);
    setLoadError(null);
    invoke<RunManifest>("get_run_manifest", { runId })
      .then((m) => {
        if (live) {
          const initial = m.artifacts.some((artifact) => artifact.rel_path === "report.md")
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
  }, [runId, manifestAttempt]);

  useEffect(() => {
    if (!manifest || !selected) return;
    let live = true;
    setContent(null);
    setLoadError(null);
    invoke<ArtifactContent>("read_artifact", { runId, relPath: selected })
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

  const entry = manifest.artifacts.find((a) => a.rel_path === selected) ?? null;

  return (
    <div className="flex h-full min-h-0">
      {/* Tree */}
      <nav
        style={{ width: treeWidth }}
        className="relative shrink-0 border-r border-gray-200 dark:border-gray-800"
      >
        <div className="h-full space-y-4 overflow-y-auto p-3">
          {GROUPS.map((g) => {
            const items = manifest.artifacts.filter((a) => a.group === g.id);
            if (items.length === 0) return null;
            return (
              <div key={g.id}>
                <h4 className="mb-1.5 text-[10px] font-semibold uppercase tracking-widest text-gray-400 dark:text-gray-500">
                  {g.label}
                </h4>
                <ul className="space-y-0.5">
                  {items.map((a) => (
                    <li key={a.rel_path}>
                      <button
                        onClick={() => setSelected(a.rel_path)}
                        title={`${a.rel_path} · ${formatBytes(a.bytes)}`}
                        className={`w-full truncate rounded px-2 py-1 text-left text-xs transition-colors ${
                          selected === a.rel_path
                            ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                            : "text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
                        }`}
                      >
                        {a.label}
                      </button>
                    </li>
                  ))}
                </ul>
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

      {/* Viewer */}
      <div className="flex-1 min-w-0 overflow-auto">
        {loadError ? (
          <div className="p-6 text-sm text-gray-500 dark:text-gray-400">
            Could not read this artifact: {loadError}
          </div>
        ) : content ? (
          <Viewer
            entry={entry}
            content={content}
            artifacts={manifest.artifacts}
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
