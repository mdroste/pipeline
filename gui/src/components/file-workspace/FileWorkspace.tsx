// Shared file tabs for project files, Review sources and imported captures.
// Each caller passes its owning adapter, and this component never widens access.
// Links resolve through lib/fileLinks.ts: relative links stay inside the owning
// root or capture, absolute links open only after the backend validates them,
// and Git or task metadata, unsafe schemes and uncaptured assets never resolve.
// Saves go through the adapter with the expected content hash. FilePane picks
// the viewer, useFileBuffers owns drafts, and FileBrowser owns the folder tree.

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
import { EmptyState, IconButton } from "../../ui";
import { Icon } from "../../ui/icons";
import type { FileAdapter } from "../../lib/fileWorkspaceClient";
import type { FileLocation } from "../../lib/fileLinks";
import type { FileMode } from "../../lib/fileKinds";
import { movedPath } from "../../lib/fileTree";
import { FileNavigationScope } from "./FileNavigation";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import FileBrowser from "./FileBrowser";
import FileIcon from "./FileIcon";
import { isDirty, useFileBuffers } from "./useFileBuffers";
import useFileChanges from "./useFileChanges";
import "./files.css";
const FilePane = lazy(() => import("./FilePane"));
export interface FilePassage {
  path: string;
  hash: string;
  quote: string;
  start: number;
  end: number;
  line: number;
  /** Set for a passage from a PDF, where offsets do not apply. */
  page?: number;
  draft: boolean;
}
interface Props {
  adapter: FileAdapter;
  paths: string[];
  initial?: FileLocation | null;
  onAsk?: (passage: FilePassage) => void;
  onAskFile?: (path: string) => void;
  onSaved?: () => void;
  onAddToDocuments?: (path: string) => Promise<void>;
}
function restoredLayout(id: string): {
  tabs: string[];
  active: FileLocation | null;
  split: string | null;
  browser: boolean;
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
        browser: v.browser !== false,
      };
  } catch {
    /* Optional layout. */
  }
  return { tabs: [], active: null, split: null, browser: true };
}
export default function FileWorkspace({
  adapter,
  paths,
  initial,
  onAsk,
  onAskFile,
  onSaved,
  onAddToDocuments,
}: Props) {
  const workspaceRoot = useRef<HTMLDivElement>(null);
  const [layout] = useState(() => restoredLayout(adapter.id));
  const [tabs, setTabs] = useState(layout.tabs);
  const [active, setActive] = useState<FileLocation | null>(
    initial ?? layout.active,
  );
  const [split, setSplit] = useState<string | null>(layout.split);
  const [history, setHistory] = useState<FileLocation[]>([]);
  const [forward, setForward] = useState<FileLocation[]>([]);
  const [browserOpen, setBrowserOpen] = useState(layout.browser);
  const search = useRef<HTMLInputElement>(null);
  const [error, setError] = useState("");
  const [passage, setPassage] = useState<FilePassage | null>(null);
  const [modes, setModes] = useState<Record<string, FileMode>>({});
  const setMode = useCallback(
    (path: string, mode: FileMode) =>
      setModes((old) => ({ ...old, [path]: mode })),
    [],
  );
  const {
    buffers,
    latest,
    saving,
    load,
    retain,
    evict,
    update,
    save,
    compareWithCurrent,
    remap,
    forget,
    release,
  } = useFileBuffers({
    adapter,
    onSaved,
    setError,
    onConflict: (path) => setMode(path, "diff"),
  });
  useFileChanges({
    adapter,
    root: workspaceRoot,
    latest,
    load,
    update,
    release,
  });
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const [failed, setFailed] = useState<ReadonlySet<string>>(new Set());
  const shown = useRef({ tabs, active, split });
  shown.current = { tabs, active, split };
  /** Take tabs away together. Returns the tab beside the first one removed. */
  const dropTabs = useCallback((paths: string[]) => {
    const gone = new Set(paths);
    const { tabs } = shown.current;
    const remaining = tabs.filter((p) => !gone.has(p));
    const first = Math.max(
      tabs.findIndex((p) => gone.has(p)),
      0,
    );
    const next = remaining[Math.min(first, remaining.length - 1)];
    setTabs((old) => old.filter((p) => !gone.has(p)));
    setActive((old) =>
      old && gone.has(old.path) ? (next ? { path: next } : null) : old,
    );
    setSplit((old) => (old && gone.has(old) ? null : old));
    return next;
  }, []);
  /** A file that is gone loses its tab; its retained draft stays in storage. */
  const loadFailed = useCallback(
    (path: string, cause: unknown) => {
      setError(workbenchErrorMessage(cause));
      if (latest.current[path]) return;
      if ((cause as { code?: unknown } | null)?.code === "file_missing")
        dropTabs([path]);
      else setFailed((old) => new Set(old).add(path));
    },
    [dropTabs, latest],
  );
  const navigationSequence = useRef(0);
  const open = useCallback(
    async (location: FileLocation) => {
      const sequence = ++navigationSequence.current;
      setError("");
      try {
        await load(location.path);
      } catch (error) {
        if (sequence === navigationSequence.current && alive.current)
          throw error;
        return;
      }
      if (sequence !== navigationSequence.current || !alive.current) return;
      if (location.line) setMode(location.path, "source");
      else if (location.fragment) setMode(location.path, "preview");
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
    [load, setMode],
  );
  const openReporting = useCallback(
    (location: FileLocation) =>
      void open(location).catch((e) => loadFailed(location.path, e)),
    [open, loadFailed],
  );
  useEffect(() => {
    if (initial) void open(initial).catch((e) => loadFailed(initial.path, e));
  }, [initial, open, loadFailed]);
  useEffect(() => {
    setFailed((old) => (old.size ? new Set() : old));
    for (const path of new Set(
      [active?.path, split].filter((p): p is string => Boolean(p)),
    ))
      void load(path).catch((e) => loadFailed(path, e));
    evict([active?.path, split]);
  }, [active?.path, split, load, evict, loadFailed]);
  useEffect(() => {
    try {
      localStorage.setItem(
        `pipeline.files.layout.${adapter.id}`,
        JSON.stringify({ tabs, active, split, browser: browserOpen }),
      );
    } catch {
      /* Layout is optional. */
    }
  }, [tabs, active, split, browserOpen, adapter.id]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "p") {
        e.preventDefault();
        setBrowserOpen(true);
        requestAnimationFrame(() => search.current?.focus());
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  const navigation = useMemo(
    () => ({
      path: active?.path ?? "",
      open,
      image: async (path: string) => {
        const file = await (adapter.readInline ?? adapter.read)(path);
        // The bytes are inline, so nothing will use a stream opened with them.
        if (file.streamToken) adapter.release?.(file.streamToken);
        if (!file.base64 || !file.mime.startsWith("image/"))
          throw new Error("This file is not a previewable image");
        return `data:${file.mime};base64,${file.base64}`;
      },
    }),
    [active?.path, adapter, open],
  );
  useEffect(() => {
    workspaceRoot.current
      ?.querySelector('.file-tab[data-active="true"]')
      ?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [active?.path]);
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
    setTabs((old) => (old.includes(next.path) ? old : [...old, next.path]));
    if (back) {
      setHistory(source.slice(0, -1));
      if (active) setForward((old) => [...old, active]);
    } else {
      setForward(source.slice(0, -1));
      if (active) setHistory((old) => [...old, active]);
    }
    setActive(next);
    if (next.line || next.fragment)
      setMode(next.path, next.line ? "source" : "preview");
    setPassage(null);
  };
  const completionTexts = useMemo(
    () =>
      Object.values(buffers)
        .filter((b) => /\.(tex|bib)$/i.test(b.file.path))
        .map((b) => b.draft),
    [buffers],
  );
  const tabList = useTabList(
    tabs,
    active?.path ?? "",
    (path) => {
      openReporting({ path });
    },
    "manual",
  );
  const closeTabs = (paths: string[]) => {
    navigationSequence.current++;
    retain(latest.current);
    const next = dropTabs(paths);
    requestAnimationFrame(() => {
      if (next) tabList.focus(next);
      else search.current?.focus();
    });
  };
  /** Tabs, panes and drafts follow a renamed file or folder. */
  const followMove = (from: string, to: string) => {
    navigationSequence.current++;
    retain(latest.current);
    const moved = remap(from, to);
    const carry = (path: string) => movedPath(path, from, to) ?? path;
    setTabs((old) => old.map(carry));
    setActive((old) => (old ? { ...old, path: carry(old.path) } : old));
    setSplit((old) => (old ? carry(old) : old));
    setHistory((old) => old.map((l) => ({ ...l, path: carry(l.path) })));
    setForward((old) => old.map((l) => ({ ...l, path: carry(l.path) })));
    setModes((old) =>
      Object.fromEntries(
        Object.entries(old).map(([path, mode]) => [carry(path), mode]),
      ),
    );
    // Open streams name the old path. A draft keeps its text and base hash.
    setTimeout(() => {
      for (const path of moved) {
        const buffer = latest.current[path];
        if (buffer && !isDirty(buffer))
          void load(path, true).catch((e) =>
            setError(workbenchErrorMessage(e)),
          );
      }
    });
  };
  // A file with unsaved text keeps its tab. One never read has nothing to show.
  const followTrash = (trashed: string) => {
    const closing = [...tabs, ...(split ? [split] : [])].filter(
      (path) =>
        movedPath(path, trashed, "") !== null &&
        (!latest.current[path] || forget(path)),
    );
    if (closing.length) closeTabs(closing);
  };
  const renderPane = (location: FileLocation) => {
    const buffer = buffers[location.path];
    if (!buffer)
      return (
        <p className="file-status">
          {failed.has(location.path)
            ? `${location.path} could not be opened.`
            : `Opening ${location.path}…`}
        </p>
      );
    const file = buffer.file;
    return (
      <FileNavigationScope
        value={{
          ...navigation,
          path: file.path,
          root: file.scopeRoot ?? undefined,
        }}
      >
        <Suspense fallback={<p className="file-status">Loading file…</p>}>
          <FilePane
            // Text from Load more belongs to one version of one file. A file
            // that can be edited keeps its editor across a save.
            key={file.truncated ? `${file.path}:${file.hash}` : file.path}
            adapter={adapter}
            location={location}
            buffer={buffer}
            mode={modes[file.path]}
            setMode={(mode) => setMode(file.path, mode)}
            saving={saving}
            completionTexts={completionTexts}
            onDraft={(draft) =>
              update(file.path, (old) =>
                old.draft === draft ? old : { ...old, draft },
              )
            }
            onDiscard={() =>
              update(file.path, (old) => ({
                ...old,
                draft: old.file.text ?? "",
              }))
            }
            onSave={() => void save(file.path)}
            onCompare={() => void compareWithCurrent(file.path)}
            onReload={() =>
              void load(file.path, "discard").catch((e) =>
                setError(workbenchErrorMessage(e)),
              )
            }
            onExternal={() =>
              void adapter
                .external?.(file.path)
                .catch((e) => setError(workbenchErrorMessage(e)))
            }
            onReveal={() =>
              void adapter
                .reveal?.(file.path)
                .catch((e) => setError(workbenchErrorMessage(e)))
            }
            onPassage={setPassage}
          />
        </Suspense>
      </FileNavigationScope>
    );
  };
  const hasBrowser = Boolean(adapter.list) || paths.length > 1;
  const files = (
    <div className="file-workspace-main">
      <div className="file-tabbar">
        {hasBrowser && (
          <IconButton
            size="sm"
            label={browserOpen ? "Hide file list" : "Show file list"}
            tooltipSide="bottom"
            onClick={() => setBrowserOpen(!browserOpen)}
          >
            <Icon name="sidebar" className="h-4 w-4" />
          </IconButton>
        )}
        {(history.length > 0 || forward.length > 0) && (
          <>
            <IconButton
              size="sm"
              label="Back"
              tooltipSide="bottom"
              disabled={!history.length}
              onClick={() => moveHistory(true)}
            >
              <Icon name="arrow-left" className="h-4 w-4" />
            </IconButton>
            <IconButton
              size="sm"
              label="Forward"
              tooltipSide="bottom"
              disabled={!forward.length}
              onClick={() => moveHistory(false)}
            >
              <Icon name="arrow" className="h-4 w-4" />
            </IconButton>
          </>
        )}
        <div className="file-tabs" role="tablist" aria-label="Open files">
          {tabs.map((path) => {
            const props = tabList.tabProps(path);
            const name = path.split("/").pop() ?? path;
            return (
              <div
                key={path}
                className="file-tab"
                role="presentation"
                data-active={active?.path === path}
              >
                <button
                  {...props}
                  className="file-tab-name"
                  title={`${path} · Delete to close`}
                  onKeyDown={(event) => {
                    if (event.key === "Delete") {
                      event.preventDefault();
                      closeTabs([path]);
                    } else props.onKeyDown(event);
                  }}
                >
                  <FileIcon name={name} />
                  <span className="file-tab-label">
                    {name}
                    {buffers[path] && isDirty(buffers[path]) ? " •" : ""}
                  </span>
                </button>
                <button
                  type="button"
                  className="file-tab-close"
                  tabIndex={active?.path === path ? 0 : -1}
                  aria-label={`Close ${path}`}
                  onClick={() => closeTabs([path])}
                >
                  <Icon name="close" className="h-3 w-3" />
                </button>
              </div>
            );
          })}
        </div>
        <div className="file-tabbar-end">
          {passage && onAsk && (
            <button
              className="file-action primary"
              onClick={() => onAsk(passage)}
            >
              Ask about selection
            </button>
          )}
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
          <IconButton
            size="sm"
            label={split ? "Close split" : "Split pane"}
            tooltipSide="bottom"
            aria-pressed={Boolean(split)}
            disabled={!active}
            onClick={() => setSplit(split ? null : (active?.path ?? null))}
          >
            <Icon name="columns" className="h-4 w-4" />
          </IconButton>
        </div>
      </div>
      {error && (
        <p role="alert" className="file-error">
          {error}
        </p>
      )}
      {tabs
        .filter((path) => path !== active?.path)
        .map((path) => (
          <div key={path} {...tabList.panelProps(path)} hidden />
        ))}
      {split && active ? (
        <SplitView
          storageKey={`pipeline.files.split.${adapter.id}`}
          firstLabel="Primary file"
          secondLabel="Second file"
          first={
            <div
              {...tabList.panelProps(active.path)}
              className="file-tab-panel"
            >
              {renderPane(active)}
            </div>
          }
          second={renderPane({ path: split })}
        />
      ) : active ? (
        <div {...tabList.panelProps(active.path)} className="file-tab-panel">
          {renderPane(active)}
        </div>
      ) : (
        <div className="file-empty">
          <EmptyState
            icon={<Icon name="file" className="h-8 w-8" />}
            title="No file open"
            description={
              hasBrowser
                ? "Choose a file from the list. Unsaved drafts stay when you switch files."
                : "Unsaved drafts stay when you switch files."
            }
          />
        </div>
      )}
    </div>
  );
  return (
    <div
      ref={workspaceRoot}
      className="file-workspace"
      aria-label="File workspace"
    >
      {hasBrowser ? (
        <div
          className="file-workspace-body"
          data-browser={browserOpen ? "open" : "closed"}
        >
          <div className="file-browser-slot" hidden={!browserOpen}>
            <FileBrowser
              adapter={adapter}
              paths={paths}
              activePath={active?.path}
              onOpen={openReporting}
              onChanged={onSaved}
              onMoved={followMove}
              onTrashed={followTrash}
              onAddToDocuments={onAddToDocuments}
              onAskFile={onAskFile}
              searchRef={search}
            />
          </div>
          {files}
        </div>
      ) : (
        files
      )}
    </div>
  );
}
