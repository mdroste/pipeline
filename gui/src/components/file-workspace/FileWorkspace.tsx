// Shared file tabs for project files, Review sources and imported captures.
// Each caller passes its owning adapter, and this component never widens access.
// Links resolve through lib/fileLinks.ts: relative links stay inside the owning
// root or capture, absolute links open only after the backend validates them,
// and Git or task metadata, unsafe schemes and uncaptured assets never resolve.
// Saves go through the adapter with the expected content hash. FilePane picks
// the viewer, useFileBuffers owns drafts, and FileBrowser owns the folder tree.
// The editor can split into two panes, each with its own tabs (fileGroups.ts);
// dragging a tab to the right or bottom of the editor makes the split.

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
  type ReactNode,
} from "react";
import { EmptyState, IconButton } from "../../ui";
import { Icon } from "../../ui/icons";
import type { FileAdapter } from "../../lib/fileWorkspaceClient";
import type { FileLocation } from "../../lib/fileLinks";
import type { FileMode } from "../../lib/fileKinds";
import type { ProjectFileDrag } from "../../lib/appEvents";
import { movedPath } from "../../lib/fileTree";
import { FileNavigationScope } from "./FileNavigation";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import FileBrowser from "./FileBrowser";
import FileIcon from "./FileIcon";
import { isDirty, useFileBuffers } from "./useFileBuffers";
import useFileChanges from "./useFileChanges";
import useTabReorder, { type TabDock } from "./useTabReorder";
import {
  closePane,
  closeTabs as closeGroupTabs,
  moveTab,
  remapLayout,
  restoreLayout,
  showIn,
  splitOff,
  type FileLayout,
  type SplitDirection,
} from "./fileGroups";
import "./files.css";
import ReportIssueLink from "../ReportIssueLink";
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
  fileListLabel?: string;
  initialFileListOpen?: boolean;
  allowSplit?: boolean;
  editOnDemand?: boolean;
  initial?: FileLocation | null;
  onAsk?: (passage: FilePassage) => void;
  onAskFile?: (path: string) => void;
  /** Run controls for the file a pane shows, such as Play for a script. */
  runControls?: (file: { path: string; dirty: boolean }) => ReactNode;
  onOpenInFiles?: (location: FileLocation) => void;
  onSaved?: () => void;
  onNavigate?: (location: FileLocation) => void;
  onAddToDocuments?: (path: string) => Promise<void>;
  /** Lets files be dragged out of the file list. */
  onFileDrag?: (drag: ProjectFileDrag) => void;
}
function restoredLayout(id: string, allowSplit: boolean) {
  let saved: unknown = null;
  try {
    saved = JSON.parse(
      localStorage.getItem(`pipeline.files.layout.${id}`) ?? "null",
    );
  } catch {
    /* Optional layout. */
  }
  return {
    files: restoreLayout(saved, allowSplit),
    browser: (saved as { browser?: unknown } | null)?.browser !== false,
  };
}
/** Where a tab dragged off its strip would land. */
type DockTarget =
  | { kind: "group"; group: number; index?: number }
  | { kind: "split"; direction: SplitDirection };
/** The area that lights up under a dragged tab. */
interface DropHint {
  group: number;
  zone: "strip" | "all" | "right" | "bottom";
}
export default function FileWorkspace({
  adapter,
  paths,
  fileListLabel,
  initialFileListOpen,
  allowSplit = true,
  editOnDemand,
  initial,
  onAsk,
  onAskFile,
  runControls,
  onOpenInFiles,
  onSaved,
  onNavigate,
  onAddToDocuments,
  onFileDrag,
}: Props) {
  const workspaceRoot = useRef<HTMLDivElement>(null);
  const navigationListener = useRef(onNavigate);
  navigationListener.current = onNavigate;
  const [layout] = useState(() => restoredLayout(adapter.id, allowSplit));
  const [files, setFiles] = useState<FileLayout>(() =>
    initial
      ? {
          ...layout.files,
          groups: layout.files.groups.map((group, i) =>
            i === layout.files.focus ? { ...group, active: initial } : group,
          ),
        }
      : layout.files,
  );
  const { groups, focus, direction } = files;
  const split = groups.length > 1;
  /** The file in the pane that files from the list or a link open into. */
  const active = groups[focus]?.active ?? null;
  const [history, setHistory] = useState<FileLocation[]>([]);
  const [forward, setForward] = useState<FileLocation[]>([]);
  const [browserOpen, setBrowserOpen] = useState(
    initialFileListOpen ?? layout.browser,
  );
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
  const shown = useRef(files);
  shown.current = files;
  /** Take tabs away together, from one pane or all. Returns the new layout. */
  const dropTabs = useCallback((paths: string[], group?: number) => {
    setFiles((old) => closeGroupTabs(old, paths, group));
    return closeGroupTabs(shown.current, paths, group);
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
    async (location: FileLocation, group?: number) => {
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
      const before = shown.current;
      const old = before.groups[group ?? before.focus]?.active;
      if (old) setHistory((h) => [...h.slice(-99), old]);
      setFiles((layout) => showIn(layout, location, group ?? layout.focus));
      setForward([]);
      setPassage(null);
      navigationListener.current?.(location);
    },
    [load, setMode],
  );
  const openReporting = useCallback(
    (location: FileLocation, group?: number) =>
      void open(location, group).catch((e) => loadFailed(location.path, e)),
    [open, loadFailed],
  );
  useEffect(() => {
    if (initial) void open(initial).catch((e) => loadFailed(initial.path, e));
  }, [initial, open, loadFailed]);
  /** The files on screen, one per pane. */
  const onScreen = groups.map((g) => g.active?.path ?? "").join("\n");
  useEffect(() => {
    setFailed((old) => (old.size ? new Set() : old));
    const paths = onScreen.split("\n").filter(Boolean);
    for (const path of new Set(paths))
      void load(path).catch((e) => loadFailed(path, e));
    evict(paths);
  }, [onScreen, load, evict, loadFailed]);
  useEffect(() => {
    try {
      localStorage.setItem(
        `pipeline.files.layout.${adapter.id}`,
        JSON.stringify({ ...files, browser: browserOpen }),
      );
    } catch {
      /* Layout is optional. */
    }
  }, [files, browserOpen, adapter.id]);
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
    for (const tab of workspaceRoot.current?.querySelectorAll(
      '.file-tab[data-active="true"]',
    ) ?? [])
      tab.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [onScreen]);
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
    setFiles((layout) => showIn(layout, next));
    if (back) {
      setHistory(source.slice(0, -1));
      if (active) setForward((old) => [...old, active]);
    } else {
      setForward(source.slice(0, -1));
      if (active) setHistory((old) => [...old, active]);
    }
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
  const firstTabs = useTabList(
    groups[0].tabs,
    groups[0].active?.path ?? "",
    (path) => openReporting({ path }, 0),
    "manual",
  );
  const secondTabs = useTabList(
    groups[1]?.tabs ?? [],
    groups[1]?.active?.path ?? "",
    (path) => openReporting({ path }, 1),
    "manual",
  );
  const tabLists = [firstTabs, secondTabs];
  const tabListsRef = useRef(tabLists);
  tabListsRef.current = tabLists;
  const focusGroup = (group: number) =>
    setFiles((old) =>
      old.focus === group || !old.groups[group]
        ? old
        : { ...old, focus: group },
    );
  // Dragging a tab off its strip: onto the other pane, or to the right or
  // bottom of the only pane to split it.
  const dockTarget = useRef<DockTarget | null>(null);
  const [dropHint, setDropHint] = useState<DropHint | null>(null);
  const dockAt = (from: number, x: number, y: number): DockTarget | null => {
    const hit = document.elementFromPoint?.(x, y);
    const strip = hit?.closest<HTMLElement>(".file-tabs[data-group]");
    if (strip) {
      const group = Number(strip.dataset.group);
      if (group === from) return null;
      const index = [
        ...strip.querySelectorAll<HTMLElement>(":scope > .file-tab"),
      ].filter((tab) => {
        const box = tab.getBoundingClientRect();
        return box.left + box.width / 2 < x;
      }).length;
      return { kind: "group", group, index };
    }
    const body = hit?.closest<HTMLElement>(".file-group-body[data-group]");
    if (!body) return null;
    const group = Number(body.dataset.group);
    if (shown.current.groups.length > 1)
      return group === from ? null : { kind: "group", group };
    const box = body.getBoundingClientRect();
    const right = (box.right - x) / box.width;
    const bottom = (box.bottom - y) / box.height;
    if (right > 0.5 && bottom > 0.5) return null;
    return {
      kind: "split",
      direction: right <= bottom ? "horizontal" : "vertical",
    };
  };
  const dockFor = (from: number): TabDock | undefined =>
    allowSplit
      ? {
          move: (_, x, y) => {
            const target = dockAt(from, x, y);
            dockTarget.current = target;
            const hint: DropHint | null = !target
              ? null
              : target.kind === "split"
                ? {
                    group: 0,
                    zone:
                      target.direction === "horizontal" ? "right" : "bottom",
                  }
                : {
                    group: target.group,
                    zone: target.index === undefined ? "all" : "strip",
                  };
            setDropHint((old) =>
              old?.group === hint?.group && old?.zone === hint?.zone
                ? old
                : hint,
            );
          },
          end: (path, drop) => {
            const target = dockTarget.current;
            dockTarget.current = null;
            setDropHint(null);
            if (!drop || !target) return;
            setFiles((old) =>
              target.kind === "split"
                ? splitOff(old, path, target.direction)
                : moveTab(old, path, from, target.group, target.index),
            );
          },
        }
      : undefined;
  const firstStrip = useRef<HTMLDivElement>(null);
  const secondStrip = useRef<HTMLDivElement>(null);
  const strips = [firstStrip, secondStrip];
  const reorders = [
    useTabReorder(
      firstStrip,
      groups[0].tabs,
      (path, index) => setFiles((old) => moveTab(old, path, 0, 0, index)),
      dockFor(0),
    ),
    useTabReorder(
      secondStrip,
      groups[1]?.tabs ?? [],
      (path, index) => setFiles((old) => moveTab(old, path, 1, 1, index)),
      dockFor(1),
    ),
  ];
  /** Closes tabs in one pane, or everywhere, and focuses the tab that takes over. */
  const closeTabs = (paths: string[], group?: number) => {
    navigationSequence.current++;
    retain(latest.current);
    const before = shown.current.groups.length;
    const after = dropTabs(paths, group);
    const index =
      group !== undefined && after.groups.length === before
        ? group
        : after.focus;
    const next = after.groups[index]?.active?.path;
    requestAnimationFrame(() => {
      if (next) tabListsRef.current[index]?.focus(next);
      else search.current?.focus();
    });
  };
  /** Tabs, panes and drafts follow a renamed file or folder. */
  const followMove = (from: string, to: string) => {
    navigationSequence.current++;
    retain(latest.current);
    const moved = remap(from, to);
    const carry = (path: string) => movedPath(path, from, to) ?? path;
    setFiles((old) => remapLayout(old, carry));
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
    const closing = [...new Set(groups.flatMap((g) => g.tabs))].filter(
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
            mode={modes[location.path]}
            setMode={(mode) => setMode(location.path, mode)}
            saving={saving}
            editOnDemand={editOnDemand}
            completionTexts={completionTexts}
            onDraft={(draft) =>
              update(location.path, (old) =>
                old.draft === draft ? old : { ...old, draft },
              )
            }
            onDiscard={() =>
              update(location.path, (old) => ({
                ...old,
                draft: old.file.text ?? "",
              }))
            }
            onSave={() => void save(location.path)}
            onCompare={() => void compareWithCurrent(location.path)}
            onReload={() =>
              void load(location.path, "discard").catch((e) =>
                setError(workbenchErrorMessage(e)),
              )
            }
            onExternal={() =>
              void adapter
                .external?.(file.path)
                .catch((e) => setError(workbenchErrorMessage(e)))
            }
            onOpenInFiles={
              onOpenInFiles
                ? () => onOpenInFiles({ ...location, path: file.path })
                : undefined
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
  const hasBrowser = Boolean(fileListLabel || adapter.list) || paths.length > 1;
  /** One pane: its tab bar, then the file it shows. */
  const groupView = (index: number) => {
    const group = groups[index];
    const shownFile = group.active;
    const tabList = tabLists[index];
    const reorder = reorders[index];
    const hint = dropHint?.group === index ? dropHint.zone : null;
    return (
      <div
        className="file-group"
        data-focused={split && index === focus}
        onPointerDownCapture={() => focusGroup(index)}
        onFocusCapture={() => focusGroup(index)}
      >
        <div className="file-tabbar">
          {index === 0 && hasBrowser && (
            <IconButton
              size="sm"
              label={browserOpen ? "Hide file list" : "Show file list"}
              tooltipSide="bottom"
              onClick={() => setBrowserOpen(!browserOpen)}
            >
              <Icon name="sidebar" className="h-4 w-4" />
            </IconButton>
          )}
          {index === 0 && (history.length > 0 || forward.length > 0) && (
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
          <div
            ref={strips[index]}
            className="file-tabs"
            role="tablist"
            aria-label={
              split
                ? `Open files, ${index ? "second" : "first"} pane`
                : "Open files"
            }
            data-group={index}
            data-drop={hint === "strip" || undefined}
          >
            {group.tabs.map((path, position) => {
              const props = tabList.tabProps(path);
              const name = path.split("/").pop() ?? path;
              return (
                <div
                  key={path}
                  className="file-tab"
                  role="presentation"
                  data-active={shownFile?.path === path}
                >
                  <button
                    {...props}
                    className="file-tab-name"
                    title={`${path} · Drag to reorder or split · Delete to close`}
                    aria-keyshortcuts="Control+Shift+PageUp Control+Shift+PageDown Delete"
                    onPointerDown={(event) =>
                      reorder.onPointerDown(path, event)
                    }
                    onKeyDown={(event) => {
                      const step =
                        (event.ctrlKey || event.metaKey) && event.shiftKey
                          ? { PageUp: -1, PageDown: 1 }[event.key]
                          : undefined;
                      if (event.key === "Delete") {
                        event.preventDefault();
                        closeTabs([path], index);
                      } else if (step) {
                        // The same keys as moving a browser or editor tab.
                        event.preventDefault();
                        setFiles((old) =>
                          moveTab(old, path, index, index, position + step),
                        );
                        requestAnimationFrame(() => tabList.focus(path));
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
                    tabIndex={shownFile?.path === path ? 0 : -1}
                    aria-label={`Close ${path}`}
                    onClick={() => closeTabs([path], index)}
                  >
                    <Icon name="close" className="h-3 w-3" />
                  </button>
                </div>
              );
            })}
          </div>
          <div className="file-tabbar-end">
            {shownFile &&
              runControls?.({
                path: shownFile.path,
                dirty:
                  !!buffers[shownFile.path] && isDirty(buffers[shownFile.path]),
              })}
            {passage && onAsk && index === focus && (
              <button
                className="file-action primary"
                onClick={() => onAsk(passage)}
              >
                Ask about selection
              </button>
            )}
            {allowSplit && !split && (
              <IconButton
                size="sm"
                label="Split pane"
                tooltipSide="bottom"
                disabled={!shownFile}
                onClick={() =>
                  setFiles((old) =>
                    old.groups[0].active
                      ? splitOff(
                          old,
                          old.groups[0].active.path,
                          "horizontal",
                          true,
                        )
                      : old,
                  )
                }
              >
                <Icon name="columns" className="h-4 w-4" />
              </IconButton>
            )}
            {split && (
              <IconButton
                size="sm"
                label="Close pane"
                tooltipSide="bottom"
                onClick={() => setFiles((old) => closePane(old, index))}
              >
                <Icon name="close" className="h-4 w-4" />
              </IconButton>
            )}
          </div>
        </div>
        {error && index === focus && (
          <p role="alert" className="file-error">
            {error}
            <ReportIssueLink />
          </p>
        )}
        {group.tabs
          .filter((path) => path !== shownFile?.path)
          .map((path) => (
            <div key={path} {...tabList.panelProps(path)} hidden />
          ))}
        <div className="file-group-body" data-group={index}>
          {shownFile ? (
            <div
              {...tabList.panelProps(shownFile.path)}
              className="file-tab-panel"
            >
              {renderPane(shownFile)}
            </div>
          ) : (
            <div className="file-empty">
              <EmptyState
                icon={<Icon name="file" className="h-8 w-8" />}
                title="No file open"
                description={[
                  hasBrowser && "Choose a file from the list.",
                  adapter.save && "Unsaved drafts stay when you switch files.",
                ]
                  .filter(Boolean)
                  .join(" ")}
              />
            </div>
          )}
          {hint && hint !== "strip" && (
            <div className="file-drop-hint" data-zone={hint} />
          )}
        </div>
      </div>
    );
  };
  const editor = (
    <div className="file-workspace-main">
      {split ? (
        <SplitView
          className="file-groups"
          storageKey={`pipeline.files.split.${adapter.id}`}
          firstLabel="First pane"
          secondLabel="Second pane"
          direction={direction}
          toolbar={false}
          first={groupView(0)}
          second={groupView(1)}
        />
      ) : (
        groupView(0)
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
              label={fileListLabel}
              paths={paths}
              activePath={active?.path}
              onOpen={openReporting}
              onChanged={onSaved}
              onMoved={followMove}
              onTrashed={followTrash}
              onAddToDocuments={onAddToDocuments}
              onAskFile={onAskFile}
              onFileDrag={onFileDrag}
              searchRef={search}
            />
          </div>
          {editor}
        </div>
      ) : (
        editor
      )}
    </div>
  );
}
