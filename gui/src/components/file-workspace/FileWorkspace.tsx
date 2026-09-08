import useTabList from "../../hooks/useTabList";
import SplitView from "../SplitView";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import type { FileAdapter, FilePreview } from "../../lib/fileWorkspaceClient";
import type { FileLocation } from "../../lib/fileLinks";
import { FileNavigationScope } from "./FileNavigation";
import SourceEditor from "./SourceEditor";
import SourceDiff from "./SourceDiff";
import ReportViewer from "../ReportViewer";
import ResearchTablePreview from "../ResearchTablePreview";
import "./files.css";
const PdfReader = lazy(() => import("./PdfReader"));
interface Buffer {
  file: FilePreview;
  draft: string;
  status: string;
}
export interface FilePassage {
  path: string;
  hash: string;
  quote: string;
  start: number;
  end: number;
  line: number;
  draft: boolean;
}
interface Props {
  adapter: FileAdapter;
  paths: string[];
  initial?: FileLocation | null;
  onAsk?: (passage: FilePassage) => void;
  onSaved?: () => void;
}
function restoredLayout(id: string): {
  tabs: string[];
  active: FileLocation | null;
  split: string | null;
} {
  try {
    const v = JSON.parse(
      localStorage.getItem(`pipeline.files.layout.${id}`) ?? "null",
    );
    if (
      v &&
      Array.isArray(v.tabs) &&
      v.tabs.every((p: unknown) => typeof p === "string") &&
      (v.active === null || typeof v.active?.path === "string")
    )
      return {
        tabs: [...new Set<string>(v.tabs)].slice(0, 30),
        active: v.active,
        split: typeof v.split === "string" ? v.split : null,
      };
  } catch {
    /* Optional layout. */
  }
  return { tabs: [], active: null, split: null };
}
export default function FileWorkspace({
  adapter,
  paths,
  initial,
  onAsk,
  onSaved,
}: Props) {
  const workspaceRoot = useRef<HTMLDivElement>(null);
  const [layout] = useState(() => restoredLayout(adapter.id));
  const [tabs, setTabs] = useState(layout.tabs);
  const [active, setActive] = useState<FileLocation | null>(
    initial ?? layout.active,
  );
  const [split, setSplit] = useState<string | null>(layout.split);
  const [buffers, setBuffers] = useState<Record<string, Buffer>>({});
  const latest = useRef(buffers);
  latest.current = buffers;
  const [history, setHistory] = useState<FileLocation[]>([]);
  const [forward, setForward] = useState<FileLocation[]>([]);
  const [query, setQuery] = useState("");
  const search = useRef<HTMLInputElement>(null);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState<string | null>(null);
  const [passage, setPassage] = useState<FilePassage | null>(null);
  const [modes, setModes] = useState<
    Record<string, "source" | "preview" | "split" | "diff">
  >({});
  const pending = useRef(new Map<string, Promise<void>>());
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const draftKey = useCallback(
    (path: string) => `pipeline.files.draft.${adapter.id}.${path}`,
    [adapter.id],
  );
  const load = useCallback(
    async (path: string, reload = false) => {
      if (!reload && latest.current[path]) return;
      if (pending.current.has(path)) return pending.current.get(path);
      const before = latest.current[path];
      const operation = (async () => {
        const file = await adapter.read(path);
        let draft = file.text ?? "";
        let base = file;
        let status = "";
        if (!reload && file.editable) {
          try {
            const retained = JSON.parse(
              localStorage.getItem(draftKey(path)) ?? "null",
            );
            if (
              typeof retained?.draft === "string" &&
              typeof retained?.file?.hash === "string" &&
              typeof retained?.file?.text === "string" &&
              retained.file.path === path
            ) {
              draft = retained.draft;
              base = {
                ...file,
                hash: retained.file.hash,
                text: retained.file.text,
              };
              status =
                file.hash === base.hash
                  ? "Restored unsaved draft"
                  : "Restored draft; the file changed externally. Compare or reload before saving.";
            }
          } catch {
            /* Invalid draft ignored. */
          }
        }
        if (alive.current)
          setBuffers((old) => ({
            ...old,
            [path]:
              reload && old[path] && old[path] !== before
                ? {
                    ...old[path],
                    status:
                      "Reload skipped because this buffer changed. Your draft was preserved.",
                  }
                : { file: base, draft, status },
          }));
      })();
      pending.current.set(path, operation);
      try {
        await operation;
      } finally {
        pending.current.delete(path);
      }
    },
    [adapter, draftKey],
  );
  const navigationSequence = useRef(0);
  const open = useCallback(
    async (location: FileLocation) => {
      const sequence = ++navigationSequence.current;
      setError("");
      try { await load(location.path); }
      catch (error) { if (sequence === navigationSequence.current && alive.current) throw error; return; }
      if (sequence !== navigationSequence.current || !alive.current) return;
      if (location.line)
        setModes((old) => ({ ...old, [location.path]: "source" }));
      else if (location.fragment)
        setModes((old) => ({ ...old, [location.path]: "preview" }));
      setTabs((old) =>
        old.includes(location.path) ? old : [...old, location.path],
      );
      setActive((old) => {
        if (old) setHistory((h) => [...h.slice(-99), old]);
        return location;
      });
      setForward([]);
      setPassage(null);
    },
    [load],
  );
  useEffect(() => {
    if (initial) void open(initial).catch((e) => setError(String(e)));
  }, [initial, open]);
  useEffect(() => {
    for (const path of new Set(
      [active?.path, split].filter((p): p is string => Boolean(p)),
    ))
      void load(path).catch((e) => setError(String(e)));
  }, [active?.path, split, load]);
  useEffect(() => {
    try {
      localStorage.setItem(
        `pipeline.files.layout.${adapter.id}`,
        JSON.stringify({ tabs, active, split }),
      );
    } catch {
      /* Layout is optional. */
    }
  }, [tabs, active, split, adapter.id]);
  const retain = useCallback(
    (values: Record<string, Buffer>) => {
      for (const [path, buffer] of Object.entries(values)) {
        if (!buffer.file.editable) continue;
        try {
          if (buffer.draft !== buffer.file.text)
            localStorage.setItem(
              draftKey(path),
              JSON.stringify({
                file: { path, hash: buffer.file.hash, text: buffer.file.text },
                draft: buffer.draft,
              }),
            );
          else localStorage.removeItem(draftKey(path));
        } catch {
          if (alive.current)
            setError(
              "Draft storage is full. Save your files before closing Pipeline.",
            );
        }
      }
    },
    [draftKey],
  );
  useEffect(() => {
    const timer = setTimeout(() => retain(buffers), 250);
    return () => clearTimeout(timer);
  }, [buffers, retain]);
  useEffect(() => {
    const flush = () => retain(latest.current);
    window.addEventListener("pagehide", flush);
    return () => {
      flush();
      window.removeEventListener("pagehide", flush);
    };
  }, [retain]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "p") {
        e.preventDefault();
        search.current?.focus();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  const save = async (path: string, replacement?: string) => {
    const buffer = latest.current[path];
    if (!buffer || !adapter.save || !buffer.file.editable || saving) return;
    setSaving(path);
    setError("");
    const text = replacement ?? buffer.draft;
    try {
      const hash = await adapter.save(buffer.file, text);
      if (!alive.current) return;
      setBuffers((old) => ({
        ...old,
        [path]: {
          ...old[path],
          file: {
            ...old[path].file,
            hash,
            text,
            bytes: new TextEncoder().encode(text).length,
          },
          draft:
            replacement !== undefined && old[path].draft === buffer.draft
              ? text
              : old[path].draft,
          status: "Saved",
        },
      }));
      onSaved?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(null);
    }
  };
  const navigation = useMemo(
    () => ({
      path: active?.path ?? "",
      open,
      image: async (path: string) => {
        const file = await adapter.read(path);
        if (!file.base64 || !file.mime.startsWith("image/"))
          throw new Error("This file is not a previewable image");
        return `data:${file.mime};base64,${file.base64}`;
      },
    }),
    [active?.path, adapter, open],
  );
  useEffect(() => {
    if (!active?.fragment || !buffers[active.path]) return;
    const frame = requestAnimationFrame(() => {
      const target = [
        ...(workspaceRoot.current?.querySelectorAll<HTMLElement>("[id]") ?? []),
      ].find(
        (e) =>
          e.dataset.sourceAnchor === active.fragment ||
          e.id === active.fragment,
      );
      if (target) target.scrollIntoView({ block: "start" });
      else setError(`Heading unavailable: ${active.fragment}`);
    });
    return () => cancelAnimationFrame(frame);
  }, [active, Boolean(active && buffers[active.path])]);
  const moveHistory = (back: boolean) => {
    const source = back ? history : forward;
    const next = source[source.length - 1];
    if (!next) return;
    navigationSequence.current++;
    setTabs(old => old.includes(next.path) ? old : [...old, next.path]);
    if (back) {
      setHistory(source.slice(0, -1));
      if (active) setForward((old) => [...old, active]);
    } else {
      setForward(source.slice(0, -1));
      if (active) setHistory((old) => [...old, active]);
    }
    setActive(next);
    if (next.line || next.fragment)
      setModes((old) => ({
        ...old,
        [next.path]: next.line ? "source" : "preview",
      }));
    setPassage(null);
  };
  const completionTexts = useMemo(
    () =>
      Object.values(buffers)
        .filter((b) => /\.(tex|bib)$/i.test(b.file.path))
        .map((b) => b.draft),
    [buffers],
  );
  const tabList = useTabList(tabs, active?.path ?? "", path => { void open({ path }).catch(e => setError(String(e))); }, "manual");
  const closeTab = (path: string) => {
    navigationSequence.current++;
    retain(latest.current);
    const remaining = tabs.filter(p => p !== path);
    const next = remaining[Math.min(tabs.indexOf(path), remaining.length - 1)];
    setTabs(remaining);
    if (active?.path === path) setActive(next ? { path: next } : null);
    if (split === path) setSplit(null);
    requestAnimationFrame(() => { if (next) tabList.focus(next); else search.current?.focus(); });
  };
  const renderPane = (location: FileLocation) => {
    const buffer = buffers[location.path];
    if (!buffer) return <p className="file-status">Opening {location.path}…</p>;
    const file = buffer.file;
    const dirty = buffer.draft !== file.text;
    const markdown = /\.(md|markdown|rmd|qmd)$/i.test(file.path);
    const previewable = markdown || file.mime === "image/svg+xml";
    const renderedPreview =
      file.mime === "image/svg+xml" && file.base64 ? (
        <img
          alt={file.path}
          src={`data:${file.mime};charset=utf-8,${encodeURIComponent(buffer.draft)}`}
        />
      ) : (
        <ReportViewer markdown={buffer.draft} />
      );
    const mode = modes[file.path] ?? (markdown ? "preview" : "source");
    const editor = (
      <SourceEditor
        value={buffer.draft}
        path={`${adapter.id}/${file.path}`}
        label={`Source editor: ${file.path}`}
        readOnly={!file.editable}
        line={location.line}
        completionTexts={completionTexts}
        onSave={() => void save(file.path)}
        onChange={(draft) =>
          setBuffers((old) =>
            old[file.path].draft === draft
              ? old
              : { ...old, [file.path]: { ...old[file.path], draft } },
          )
        }
        onSelection={(s) =>
          setPassage(
            s.text
              ? {
                  path: file.path,
                  hash: file.hash,
                  quote: s.text,
                  start: new TextEncoder().encode(
                    buffer.draft.slice(0, s.start),
                  ).length,
                  end: new TextEncoder().encode(buffer.draft.slice(0, s.end))
                    .length,
                  line: s.line,
                  draft: dirty,
                }
              : null,
          )
        }
      />
    );
    return (
      <FileNavigationScope value={{ ...navigation, path: file.path }}>
        <div className="file-pane" tabIndex={0}>
          <div className="file-toolbar">
            <strong className="mr-auto break-all">
              {file.path}
              {dirty && file.editable ? " •" : ""}
            </strong>
            {file.text !== null && (
              <>
                {(previewable
                  ? ["source", "preview", "split", "diff"]
                  : file.editable
                    ? ["source", "diff"]
                    : ["source"]
                ).map((m) => (
                  <button
                    key={m}
                    aria-pressed={mode === m}
                    onClick={() =>
                      setModes((old) => ({
                        ...old,
                        [file.path]: m as typeof mode,
                      }))
                    }
                  >
                    {m === "diff" ? "Changes" : m[0].toUpperCase() + m.slice(1)}
                  </button>
                ))}
              </>
            )}
            {file.editable && (
              <>
                <button
                  disabled={!dirty || saving !== null}
                  onClick={() => void save(file.path)}
                >
                  {saving === file.path ? "Saving…" : "Save"}
                </button>
                <button
                  disabled={!dirty}
                  onClick={() =>
                    setBuffers((old) => ({
                      ...old,
                      [file.path]: {
                        ...old[file.path],
                        draft: old[file.path].file.text ?? "",
                      },
                    }))
                  }
                >
                  Discard draft
                </button>
              </>
            )}
            <button
              disabled={dirty && file.editable}
              onClick={() =>
                void load(file.path, true).catch((e) => setError(String(e)))
              }
            >
              Reload
            </button>
            {(adapter.external || file.externalPath) && (
              <button
                onClick={() =>
                  void (
                    adapter.external
                      ? adapter.external(file.path)
                      : openExternal(file.externalPath!)
                  ).catch((e) => setError(String(e)))
                }
              >
                Open externally
              </button>
            )}
            {adapter.reveal && (
              <button
                onClick={() =>
                  void adapter.reveal!(file.path).catch((e) =>
                    setError(String(e)),
                  )
                }
              >
                Reveal in folder
              </button>
            )}
          </div>
          {file.truncated && (
            <p role="status" className="file-status">
              This file preview is truncated. Editing is disabled.
            </p>
          )}
          {buffer.status && (
            <p role="status" className="file-status">
              {buffer.status}
            </p>
          )}
          {file.mime === "application/pdf" && file.base64 ? (
            <Suspense fallback={<p>Loading PDF reader…</p>}>
              <PdfReader
                documentKey={`${adapter.id}:${file.path}:${file.hash}`}
                title={file.path}
                initialPage={location.page}
                load={() => Promise.resolve(file.base64!)}
              />
            </Suspense>
          ) : file.mime.startsWith("image/") &&
            file.base64 &&
            file.text === null ? (
            <img
              alt={file.path}
              src={`data:${file.mime};base64,${file.base64}`}
              className="max-w-full self-start"
            />
          ) : file.text === null ? (
            <p className="file-status">
              Binary file · {file.bytes.toLocaleString()} bytes.
            </p>
          ) : /\.(csv|tsv)$/i.test(file.path) && !file.editable ? (
            <ResearchTablePreview text={file.text} path={file.path} />
          ) : mode === "diff" ? (
            <SourceDiff
              before={file.text}
              after={buffer.draft}
              path={file.path}
            />
          ) : mode === "source" ? (
            editor
          ) : mode === "preview" ? (
            <div className="min-h-0 flex-1 overflow-auto">
              {renderedPreview}
            </div>
          ) : (
            <SplitView key={`${adapter.id}:${file.path}`} storageKey={`pipeline.files.sourceSplit.${adapter.id}.${file.path}`} firstLabel="Source" secondLabel="Preview" first={editor} second={renderedPreview} />
          )}
        </div>
      </FileNavigationScope>
    );
  };
  return (
    <div
      ref={workspaceRoot}
      className="file-workspace"
      aria-label="File workspace"
    >
      <div className="file-toolbar">
        <button disabled={!history.length} onClick={() => moveHistory(true)}>
          Back
        </button>
        <button disabled={!forward.length} onClick={() => moveHistory(false)}>
          Forward
        </button>
        <input
          ref={search}
          aria-label="Quick open file"
          placeholder="Find a file… ⌘/Ctrl+P"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && query.trim()) {
              const path =
                paths.find((p) =>
                  p.toLowerCase().includes(query.toLowerCase()),
                ) ?? query;
              void open({ path }).catch((e) => setError(String(e)));
              setQuery("");
            }
          }}
        />
        <button
          disabled={!active}
          onClick={() => setSplit(split ? null : (active?.path ?? null))}
        >
          {split ? "Close split" : "Split pane"}
        </button>
        {split && (
          <select
            aria-label="File in second pane"
            value={split}
            onChange={(e) => setSplit(e.target.value)}
          >
            {[...new Set([...tabs, ...paths])].map((path) => (
              <option key={path}>{path}</option>
            ))}
          </select>
        )}
        {passage && onAsk && (
          <button onClick={() => onAsk(passage)}>Ask about selection</button>
        )}
      </div>
      {query && (
        <div className="max-h-44 overflow-auto border-b p-2">
          {paths
            .filter((path) => path.toLowerCase().includes(query.toLowerCase()))
            .slice(0, 100)
            .map((path) => (
              <button
                className="block w-full p-1 text-left font-mono text-xs"
                key={path}
                onClick={() => {
                  void open({ path }).catch((e) => setError(String(e)));
                  setQuery("");
                }}
              >
                {path}
              </button>
            ))}
        </div>
      )}
      <div className="file-tabs" role="tablist" aria-label="Open files">
        {tabs.map(path => {
          const props = tabList.tabProps(path);
          return <div key={path} className="file-tab" role="presentation" data-active={active?.path === path}>
            <button {...props} title={`${path} · Delete to close`} onKeyDown={event => {
              if (event.key === "Delete") { event.preventDefault(); closeTab(path); }
              else props.onKeyDown(event);
            }}>
              {path.split("/").pop()}{buffers[path]?.file.editable && buffers[path].draft !== buffers[path].file.text ? " •" : ""}
            </button>
            <button type="button" tabIndex={active?.path === path ? 0 : -1} aria-label={`Close ${path}`} onClick={() => closeTab(path)}>×</button>
          </div>;
        })}
      </div>
      {error && (
        <p role="alert" className="file-error">
          {error}
        </p>
      )}
      {tabs.filter(path => path !== active?.path).map(path => <div key={path} {...tabList.panelProps(path)} hidden />)}
      {split && active ? (
        <SplitView storageKey={`pipeline.files.split.${adapter.id}`} firstLabel="Primary file" secondLabel="Second file"
          first={<div {...tabList.panelProps(active.path)} className="file-tab-panel">{renderPane(active)}</div>}
          second={renderPane({ path: split })} />
      ) : active ? (
        <div {...tabList.panelProps(active.path)} className="file-tab-panel">{renderPane(active)}</div>
      ) : <p className="file-status">Find a file above to open it. Unsaved drafts are retained when switching files.</p>}

    </div>
  );
}
