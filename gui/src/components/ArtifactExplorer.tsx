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

function Viewer({
  entry,
  content,
}: {
  entry: ArtifactEntry | null;
  content: ArtifactContent;
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
      <nav className="w-56 shrink-0 border-r border-gray-200 dark:border-gray-800 overflow-y-auto p-3 space-y-4">
        {GROUPS.map((g) => {
          const items = manifest.artifacts.filter((a) => a.group === g.id);
          if (items.length === 0) return null;
          return (
            <div key={g.id}>
              <h4 className="text-[10px] font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-widest mb-1.5">
                {g.label}
              </h4>
              <ul className="space-y-0.5">
                {items.map((a) => (
                  <li key={a.rel_path}>
                    <button
                      onClick={() => setSelected(a.rel_path)}
                      title={`${a.rel_path} · ${formatBytes(a.bytes)}`}
                      className={`w-full text-left text-xs px-2 py-1 rounded truncate transition-colors ${
                        selected === a.rel_path
                          ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                          : "text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800"
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
      </nav>

      {/* Viewer */}
      <div className="flex-1 min-w-0 overflow-auto">
        {loadError ? (
          <div className="p-6 text-sm text-gray-500 dark:text-gray-400">
            Could not read this artifact: {loadError}
          </div>
        ) : content ? (
          <Viewer entry={entry} content={content} />
        ) : (
          <div className="flex items-center justify-center h-full text-gray-400">
            <div className="animate-spin w-5 h-5 border-2 border-gray-300 border-t-gray-600 rounded-full" />
          </div>
        )}
      </div>
    </div>
  );
}
