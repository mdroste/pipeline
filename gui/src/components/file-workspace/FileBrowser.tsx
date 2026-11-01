// The folder tree beside the file tabs. Folders load one at a time, so a project
// of any size opens at once. Adapters without `list` get a tree of their fixed
// paths. Each change goes through the adapter's owner, and the tree holds no
// file contents. Its one search box finds files by name and opens the first
// match on Enter. Actions on a row live in that row's menu, opened by its
// three-dot button, a right-click, or the context-menu key.
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";
import type {
  FileAdapter,
  FileListEntry,
  FileListing,
  FileSearchMatch,
  FileSearchResult,
} from "../../lib/fileWorkspaceClient";
import { formatFileSize } from "../../lib/fileKinds";
import { onFileDrop } from "../../lib/fileDrop";
import {
  baseName,
  invalidEntryName,
  joinPath,
  listingFromPaths,
  parentFolder,
  visibleRows,
} from "../../lib/fileTree";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { isMac } from "../../lib/platform";
import { confirmDialog, notify, promptDialog } from "../DialogService";
import type { FileLocation } from "../../lib/fileLinks";
import { IconButton, Menu, MenuItem, MenuSeparator } from "../../ui";
import { Icon } from "../../ui/icons";
import FileIcon from "./FileIcon";

const POLL_MS = 2000;
const MAX_FILTERED = 200;
const MAX_POLLED = 64;
const MORE_ITEMS =
  "This folder has more than 5,000 items, so only the first 5,000 are listed, folders first. Open the folder on your computer to see the rest.";

function restoredExpanded(id: string): Set<string> {
  try {
    const saved = JSON.parse(
      localStorage.getItem(`pipeline.files.tree.${id}`) ?? "[]",
    );
    return new Set(
      Array.isArray(saved)
        ? saved.filter((p): p is string => typeof p === "string").slice(0, 200)
        : [],
    );
  } catch {
    return new Set();
  }
}
export default function FileBrowser({
  adapter,
  paths,
  activePath,
  onOpen,
  onChanged,
  onMoved,
  onTrashed,
  onAddToDocuments,
  onAskFile,
  searchRef,
}: {
  adapter: FileAdapter;
  paths: string[];
  activePath?: string;
  onOpen: (location: FileLocation) => void;
  onChanged?: () => void;
  onMoved?: (from: string, to: string) => void;
  onTrashed?: (path: string) => void;
  onAddToDocuments?: (path: string) => Promise<void>;
  onAskFile?: (path: string) => void;
  searchRef?: RefObject<HTMLInputElement | null>;
}) {
  const root = useRef<HTMLDivElement>(null);
  const [listings, setListings] = useState<Record<string, FileListing>>({});
  const latest = useRef(listings);
  latest.current = listings;
  const [expanded, setExpanded] = useState(() => restoredExpanded(adapter.id));
  const expandedRef = useRef(expanded);
  expandedRef.current = expanded;
  const [selected, setSelected] = useState("");
  const [filter, setFilter] = useState("");
  const [searching, setSearching] = useState(false);
  const [found, setFound] = useState<FileSearchResult | null>(null);
  const [error, setError] = useState("");
  const [dropping, setDropping] = useState(false);
  const [addMenu, setAddMenu] = useState(false);
  const addButton = useRef<HTMLButtonElement>(null);
  const [rowMenu, setRowMenu] = useState<{
    path: string;
    x: number;
    y: number;
    pointer: boolean;
  } | null>(null);
  const menuAnchor = useRef<HTMLSpanElement>(null);
  const focusFrame = useRef(0);
  const stamps = useRef<Record<string, string | null>>({});
  const fixedPaths = useRef(paths);
  fixedPaths.current = paths;
  const read = useCallback(
    async (directory: string) => {
      const listing = adapter.list
        ? await adapter.list(directory)
        : listingFromPaths(fixedPaths.current, directory);
      setListings((old) =>
        JSON.stringify(old[directory]) === JSON.stringify(listing)
          ? old
          : { ...old, [directory]: listing },
      );
      return listing;
    },
    [adapter],
  );
  // Callers build `paths` inline, so only a real change may list folders again,
  // and only a tree built from it depends on it.
  const treeKey = adapter.list ? "" : JSON.stringify(paths);
  const refresh = useCallback(
    async (directories?: string[]) => {
      const wanted = directories ?? Object.keys(latest.current);
      await Promise.all(
        wanted.map((directory) =>
          read(directory).catch(() => {
            // A folder that left the project collapses out of the tree.
            setListings((old) => {
              const next = { ...old };
              delete next[directory];
              return next;
            });
          }),
        ),
      );
    },
    [read],
  );
  useEffect(() => {
    setError("");
    void read("").catch((e) => setError(workbenchErrorMessage(e)));
    for (const directory of expandedRef.current)
      void read(directory).catch(() => undefined);
  }, [read, treeKey]);
  useEffect(() => {
    try {
      localStorage.setItem(
        `pipeline.files.tree.${adapter.id}`,
        JSON.stringify([...expanded]),
      );
    } catch {
      /* The tree layout is optional. */
    }
  }, [expanded, adapter.id]);
  // Folders change their stamp when an entry is added, removed or renamed.
  useEffect(() => {
    const stat = adapter.stat;
    if (!stat || !adapter.list) return;
    let running = false;
    const check = async () => {
      const element = root.current;
      if (
        running ||
        document.visibilityState === "hidden" ||
        !element ||
        element.closest("[hidden]")
      )
        return;
      const folders = Object.keys(latest.current).slice(0, MAX_POLLED);
      running = true;
      try {
        const result = await stat(folders);
        // A folder's first stamp lists it once more, in case it changed since
        // its first listing.
        const changed = result.filter(
          ({ path, stamp }) => stamps.current[path] !== stamp,
        );
        for (const { path, stamp } of result) stamps.current[path] = stamp;
        if (changed.length) await refresh(changed.map((c) => c.path));
      } catch {
        /* The next check retries. */
      } finally {
        running = false;
      }
    };
    const timer = setInterval(() => void check(), POLL_MS);
    return () => clearInterval(timer);
  }, [adapter, read, refresh]);
  const rows = useMemo(
    () => visibleRows(listings, expanded),
    [listings, expanded],
  );
  const filtered = useMemo(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return [];
    const candidates = new Set(paths);
    for (const listing of Object.values(listings))
      for (const entry of listing.entries)
        if (entry.kind === "file") candidates.add(entry.path);
    return [...candidates]
      .filter((path) => path.toLowerCase().includes(query))
      .slice(0, MAX_FILTERED);
  }, [filter, paths, listings]);
  const toggle = (path: string) => {
    const next = new Set(expanded);
    if (next.delete(path)) {
      setExpanded(next);
      return;
    }
    next.add(path);
    setExpanded(next);
    void read(path).catch((e) => setError(workbenchErrorMessage(e)));
  };
  const entryFor = (path: string): FileListEntry | undefined =>
    latest.current[parentFolder(path)]?.entries.find((e) => e.path === path);
  const selectedEntry = selected ? entryFor(selected) : undefined;
  /** The folder new items go in: the selection, or the one holding it. */
  const targetFolder = () =>
    selectedEntry?.kind === "folder"
      ? selectedEntry.path
      : parentFolder(selected);
  const run = async (action: () => Promise<void>, folders: string[]) => {
    setError("");
    try {
      await action();
      await refresh([...new Set(folders)]);
      onChanged?.();
      return true;
    } catch (e) {
      setError(workbenchErrorMessage(e));
      return false;
    }
  };
  const named = async (message: string, initial = "") => {
    const value = await promptDialog(message, {
      defaultValue: initial,
    });
    if (value === null) return null;
    const problem = invalidEntryName(value);
    if (problem) {
      setError(problem);
      return null;
    }
    setError("");
    return value.trim();
  };
  const reveal = (path: string) => {
    const folder = parentFolder(path);
    setExpanded((old) => {
      const next = new Set(old);
      for (let parent = folder; parent; parent = parentFolder(parent))
        next.add(parent);
      return next;
    });
    setSelected(path);
  };
  const createFile = async () => {
    const folder = targetFolder();
    const name = await named(
      folder ? `New file in ${folder}` : "New file in this project",
    );
    if (!name) return;
    const path = joinPath(folder, name);
    if (await run(() => adapter.createFile!(path), [folder])) {
      reveal(path);
      onOpen({ path });
    }
  };
  const createFolder = async () => {
    const folder = targetFolder();
    const name = await named(
      folder ? `New folder in ${folder}` : "New folder in this project",
    );
    if (!name) return;
    const path = joinPath(folder, name);
    if (await run(() => adapter.createFolder!(path), [folder])) reveal(path);
  };
  const move = async (from: string, to: string) => {
    const moved = await run(
      () => adapter.move!(from, to),
      [parentFolder(from), parentFolder(to)],
    );
    if (!moved) return;
    onMoved?.(from, to);
    reveal(to);
    notify(`Moved to ${to}`, "info", {
      label: "Undo",
      run: () => {
        void run(
          () => adapter.move!(to, from),
          [parentFolder(from), parentFolder(to)],
        ).then((undone) => {
          if (undone) {
            onMoved?.(to, from);
            reveal(from);
          }
        });
      },
    });
  };
  const rename = async (path = selected) => {
    const name = await named(`Rename ${path}`, baseName(path));
    if (name && name !== baseName(path))
      await move(path, joinPath(parentFolder(path), name));
  };
  const moveTo = async (path = selected) => {
    const target = await promptDialog(
      `Move ${baseName(path)} to folder (blank for the project's top folder)`,
      { defaultValue: parentFolder(path) },
    );
    if (target === null) return;
    const folder = target.trim().replace(/^\/+|\/+$/g, "");
    const to = joinPath(folder, baseName(path));
    if (to !== path) await move(path, to);
  };
  const trash = async (path = selected) => {
    const folder = entryFor(path)?.kind === "folder";
    const confirmed = await confirmDialog(
      `Move ${path} to the Trash?${folder ? " Everything inside it goes too." : ""}`,
      {
        title: "Move to Trash",
        confirmLabel: "Move to Trash",
        destructive: true,
      },
    );
    if (!confirmed) return;
    if (await run(() => adapter.trash!(path), [parentFolder(path)])) {
      setSelected("");
      onTrashed?.(path);
    }
  };
  const importInto = useCallback(
    async (folder: string, sources: string[]) => {
      if (!adapter.importFiles || !sources.length) return;
      setError("");
      try {
        await adapter.importFiles(folder, sources);
        await refresh([folder]);
        onChanged?.();
      } catch (e) {
        setError(workbenchErrorMessage(e));
      }
    },
    [adapter, refresh, onChanged],
  );
  const addFiles = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const chosen = await open({ multiple: true, directory: false });
    if (!chosen) return;
    await importInto(targetFolder(), Array.isArray(chosen) ? chosen : [chosen]);
  };
  const importRef = useRef(importInto);
  importRef.current = importInto;
  const folderRef = useRef(targetFolder);
  folderRef.current = targetFolder;
  useEffect(() => {
    if (!adapter.importFiles) return;
    let disposed = false;
    let stop: (() => void) | null = null;
    const inside = (x: number, y: number) => {
      const element = root.current;
      const hit = document.elementFromPoint?.(x, y);
      return Boolean(element && hit && element.contains(hit));
    };
    void onFileDrop((event) => {
      if (event.type === "leave") setDropping(false);
      else if (event.type === "over") setDropping(inside(event.x, event.y));
      else {
        setDropping(false);
        if (inside(event.x, event.y))
          void importRef.current(folderRef.current(), event.paths);
      }
    }).then((unsubscribe) => {
      if (disposed) unsubscribe();
      else stop = unsubscribe;
    });
    return () => {
      disposed = true;
      stop?.();
    };
  }, [adapter.importFiles]);
  const searchContents = async () => {
    const query = filter.trim();
    if (!adapter.search || query.length < 2) return;
    setSearching(true);
    setError("");
    try {
      setFound(await adapter.search(query));
    } catch (e) {
      setError(workbenchErrorMessage(e));
    } finally {
      setSearching(false);
    }
  };
  const focusRow = (path: string) => {
    focusFrame.current = requestAnimationFrame(() =>
      root.current
        ?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)
        ?.focus(),
    );
  };
  const hasRowMenu = (entry: FileListEntry) =>
    (entry.kind === "file" || entry.kind === "folder") &&
    Boolean(
      adapter.move ||
      adapter.trash ||
      (entry.kind === "file" && (onAddToDocuments || onAskFile)),
    );
  const openRowMenu = (
    path: string,
    x: number,
    y: number,
    pointer: boolean,
  ) => {
    // Focus handed back to a row now would close the menu opening here.
    cancelAnimationFrame(focusFrame.current);
    setSelected(path);
    setRowMenu({ path, x, y, pointer });
  };
  const closeRowMenu = () => {
    const path = rowMenu?.path;
    setRowMenu(null);
    // A chosen action may have opened a dialog, which keeps the focus.
    focusFrame.current = requestAnimationFrame(() => {
      const focused = document.activeElement;
      if (path && (focused === menuAnchor.current || focused === document.body))
        focusRow(path);
    });
  };
  /** Show a file found by name in the tree, and open it. */
  const chooseMatch = (path: string) => {
    setFilter("");
    setFound(null);
    reveal(path);
    for (let parent = parentFolder(path); parent; parent = parentFolder(parent))
      if (!latest.current[parent]) void read(parent).catch(() => undefined);
    onOpen({ path });
  };
  const onRowKey = (
    event: React.KeyboardEvent,
    entry: FileListEntry,
    index: number,
  ) => {
    const go = (next: number) => {
      const row = rows[next];
      if (!row) return;
      event.preventDefault();
      setSelected(row.entry.path);
      focusRow(row.entry.path);
    };
    const folder = entry.kind === "folder";
    if (event.key === "ArrowDown") go(index + 1);
    else if (event.key === "ArrowUp") go(index - 1);
    else if (event.key === "Home") go(0);
    else if (event.key === "End") go(rows.length - 1);
    else if (event.key === "ArrowRight" && folder) {
      event.preventDefault();
      if (!expanded.has(entry.path)) toggle(entry.path);
      else go(index + 1);
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      if (folder && expanded.has(entry.path)) toggle(entry.path);
      else {
        const parent = parentFolder(entry.path);
        if (parent) {
          setSelected(parent);
          focusRow(parent);
        }
      }
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      activate(entry);
    } else if (
      (event.key === "ContextMenu" ||
        (event.key === "F10" && event.shiftKey)) &&
      hasRowMenu(entry)
    ) {
      event.preventDefault();
      const box = event.currentTarget.getBoundingClientRect();
      openRowMenu(entry.path, box.right, box.bottom, false);
    } else if (event.key === "F2" && adapter.move) {
      event.preventDefault();
      setSelected(entry.path);
      void rename(entry.path);
    } else if (
      (event.key === "Delete" ||
        (event.key === "Backspace" && event.metaKey)) &&
      adapter.trash
    ) {
      event.preventDefault();
      setSelected(entry.path);
      void trash(entry.path);
    }
  };
  const activate = (entry: FileListEntry) => {
    setSelected(entry.path);
    if (entry.kind === "folder") toggle(entry.path);
    else if (entry.kind === "file") onOpen({ path: entry.path });
  };
  const menuEntry = rowMenu ? entryFor(rowMenu.path) : undefined;
  const menuIsFile = menuEntry?.kind === "file";
  const canAdd = adapter.createFile || adapter.createFolder;
  return (
    <div
      ref={root}
      className="file-browser"
      aria-label="Project folder"
      role="region"
      data-dropping={dropping}
    >
      <div className="file-browser-tools">
        <label className="file-search">
          <Icon name="search" className="h-3.5 w-3.5" />
          <input
            ref={searchRef}
            aria-label="Find a file"
            placeholder="Find a file"
            value={filter}
            onChange={(e) => {
              setFilter(e.target.value);
              setFound(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape" && filter) {
                e.stopPropagation();
                setFilter("");
                setFound(null);
              } else if (e.key === "Enter") {
                if (filtered[0]) chooseMatch(filtered[0]);
                else void searchContents();
              }
            }}
          />
          {!filter && <kbd>{isMac ? "⌘P" : "Ctrl+P"}</kbd>}
        </label>
        {canAdd && (
          <>
            <IconButton
              ref={addButton}
              size="sm"
              label="Add to this folder"
              tooltip={addMenu ? null : "New file, new folder, or add files"}
              tooltipSide="bottom"
              aria-haspopup="menu"
              aria-expanded={addMenu}
              onClick={() => setAddMenu(!addMenu)}
            >
              <Icon name="plus" className="h-4 w-4" />
            </IconButton>
            <Menu
              open={addMenu}
              onClose={() => setAddMenu(false)}
              anchorRef={addButton}
              label="Add to this folder"
            >
              {adapter.createFile && (
                <MenuItem onSelect={() => void createFile()}>New file</MenuItem>
              )}
              {adapter.createFolder && (
                <MenuItem onSelect={() => void createFolder()}>
                  New folder
                </MenuItem>
              )}
              {adapter.importFiles && (
                <>
                  <MenuSeparator />
                  <MenuItem onSelect={() => void addFiles()}>
                    Add files
                  </MenuItem>
                </>
              )}
            </Menu>
          </>
        )}
      </div>
      {error && (
        <p role="alert" className="file-error">
          {error}
        </p>
      )}
      {filter.trim() ? (
        <div className="file-browser-list">
          <ul aria-label="Matching files">
            {filtered.map((path) => (
              <li key={path}>
                <button
                  className="file-match"
                  aria-label={path}
                  title={path}
                  onClick={() => chooseMatch(path)}
                >
                  <FileIcon name={path} />
                  <span className="file-browser-name">{baseName(path)}</span>
                  <span className="file-match-folder">
                    {parentFolder(path)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
          {!filtered.length && (
            <p className="file-status">No file names match.</p>
          )}
          {adapter.search && (
            <button
              className="file-browser-search"
              disabled={searching || filter.trim().length < 2}
              onClick={() => void searchContents()}
            >
              {searching ? "Searching…" : "Search inside files"}
            </button>
          )}
          {found && (
            <SearchResults
              result={found}
              onOpen={({ path, line }) => onOpen({ path, line })}
            />
          )}
        </div>
      ) : (
        <ul
          role="tree"
          aria-label="Project files"
          className="file-browser-tree"
        >
          {rows.map(({ entry, depth }, index) => {
            const folder = entry.kind === "folder";
            const open = expanded.has(entry.path);
            const current = selected ? selected === entry.path : index === 0;
            return (
              <li
                key={entry.path}
                role="treeitem"
                aria-level={depth + 1}
                aria-expanded={folder ? open : undefined}
                aria-selected={selected === entry.path}
                data-path={entry.path}
                data-active={activePath === entry.path}
                data-ignored={entry.ignored}
                tabIndex={current ? 0 : -1}
                title={
                  entry.kind === "link"
                    ? `${entry.path} · link, not followed`
                    : entry.kind === "file" && entry.size
                      ? `${entry.path} · ${formatFileSize(entry.size)}`
                      : entry.path
                }
                style={{ paddingLeft: 4 + depth * 14 }}
                onClick={() => activate(entry)}
                onContextMenu={(event) => {
                  if (!hasRowMenu(entry)) return;
                  event.preventDefault();
                  openRowMenu(entry.path, event.clientX, event.clientY, true);
                }}
                onKeyDown={(event) => onRowKey(event, entry, index)}
              >
                <span
                  aria-hidden="true"
                  className="file-browser-mark"
                  data-open={folder && open}
                >
                  {folder && <Icon name="chevron-right" className="h-3 w-3" />}
                </span>
                {folder ? (
                  <span className="file-icon" data-type="folder">
                    <Icon name="folder" className="h-4 w-4" />
                  </span>
                ) : entry.kind === "file" ? (
                  <FileIcon name={entry.name} />
                ) : (
                  <span className="file-icon">
                    <Icon name="link" className="h-4 w-4" />
                  </span>
                )}
                <span className="file-browser-name">{entry.name}</span>
                {folder && open && listings[entry.path]?.truncated && (
                  <span className="file-browser-note" title={MORE_ITEMS}>
                    First 5,000 items
                  </span>
                )}
                {hasRowMenu(entry) && (
                  // Keyboard and screen reader users reach this menu with the
                  // context-menu key, so the row keeps its one tab stop.
                  <button
                    type="button"
                    aria-hidden="true"
                    tabIndex={-1}
                    className="file-browser-row-menu"
                    data-open={rowMenu?.path === entry.path}
                    onClick={(event) => {
                      event.stopPropagation();
                      const box = event.currentTarget.getBoundingClientRect();
                      openRowMenu(entry.path, box.right, box.bottom, false);
                    }}
                  >
                    <Icon name="more" className="h-4 w-4" />
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {!filter.trim() && listings[""]?.truncated && (
        <p className="file-status">{MORE_ITEMS}</p>
      )}
      {!filter.trim() && listings[""] && !rows.length && (
        <p className="file-status">This folder is empty.</p>
      )}
      <span
        ref={menuAnchor}
        tabIndex={-1}
        className="file-menu-anchor"
        style={{ left: rowMenu?.x ?? 0, top: rowMenu?.y ?? 0 }}
      />
      <Menu
        open={Boolean(rowMenu)}
        onClose={closeRowMenu}
        anchorRef={menuAnchor}
        label={`Actions for ${rowMenu ? baseName(rowMenu.path) : "file"}`}
        align={rowMenu?.pointer ? "start" : "end"}
      >
        {rowMenu && menuIsFile && onAskFile && (
          <MenuItem onSelect={() => onAskFile(rowMenu.path)}>
            Ask about this
          </MenuItem>
        )}
        {rowMenu && menuIsFile && onAddToDocuments && (
          <MenuItem
            onSelect={() =>
              void onAddToDocuments(rowMenu.path).catch((e) =>
                setError(workbenchErrorMessage(e)),
              )
            }
          >
            Add to Documents
          </MenuItem>
        )}
        {rowMenu &&
          menuIsFile &&
          (onAskFile || onAddToDocuments) &&
          (adapter.move || adapter.trash) && <MenuSeparator />}
        {rowMenu && adapter.move && (
          <MenuItem onSelect={() => void rename(rowMenu.path)}>Rename</MenuItem>
        )}
        {rowMenu && adapter.move && (
          <MenuItem onSelect={() => void moveTo(rowMenu.path)}>Move</MenuItem>
        )}
        {rowMenu && adapter.trash && (
          <MenuItem danger onSelect={() => void trash(rowMenu.path)}>
            Move to Trash
          </MenuItem>
        )}
      </Menu>
    </div>
  );
}
function SearchResults({
  result,
  onOpen,
}: {
  result: FileSearchResult;
  onOpen: (match: FileSearchMatch) => void;
}) {
  return (
    <div className="file-browser-results">
      <p role="status" className="file-status">
        {result.matches.length
          ? `${result.matches.length}${result.truncated ? "+" : ""} matches in ${result.searchedFiles} files`
          : `No matches in ${result.searchedFiles} files`}
        {result.skippedFiles ? ` · ${result.skippedFiles} files skipped` : ""}
      </p>
      <ul aria-label="Search results">
        {result.matches.map((match, index) => (
          <li key={index}>
            <button onClick={() => onOpen(match)} title={match.path}>
              <span>
                {match.path}:{match.line}
              </span>
              <code>{match.text}</code>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
