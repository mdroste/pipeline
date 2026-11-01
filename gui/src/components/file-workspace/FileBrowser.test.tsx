import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import FileBrowser from "./FileBrowser";
import type { FileAdapter, FileListEntry } from "../../lib/fileWorkspaceClient";

const dialogs = vi.hoisted(() => ({
  confirmDialog: vi.fn(),
  promptDialog: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../DialogService", () => dialogs);
vi.mock("../../lib/fileDrop", () => ({
  onFileDrop: async () => () => undefined,
}));

const entry = (
  path: string,
  kind: FileListEntry["kind"] = "file",
  ignored = false,
): FileListEntry => ({
  name: path.split("/").pop()!,
  path,
  kind,
  size: 10,
  modifiedMs: 0,
  ignored,
  executable: false,
});
function adapter(overrides: Partial<FileAdapter> = {}): FileAdapter {
  const tree: Record<string, FileListEntry[]> = {
    "": [
      entry("code", "folder"),
      entry("target", "folder", true),
      entry("escape", "link"),
      entry("a.txt"),
    ],
    code: [entry("code/run.do")],
  };
  return {
    id: "test",
    read: vi.fn(),
    list: vi.fn(async (directory) => ({
      directory,
      entries: tree[directory] ?? [],
      truncated: false,
    })),
    createFile: vi.fn(async () => undefined),
    createFolder: vi.fn(async () => undefined),
    move: vi.fn(async () => undefined),
    trash: vi.fn(async () => undefined),
    ...overrides,
  };
}
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
});
afterEach(() => vi.useRealTimers());
const stamped = (paths: string[]) =>
  paths.map((path) => ({
    path,
    kind: "folder" as const,
    stamp: "stamp",
    size: 0,
    modifiedMs: 0,
  }));
const intervals = (count: number) =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(count * 2100);
  });
it("lists folders once however often the parent renders", async () => {
  const files = adapter();
  const onOpen = vi.fn();
  const { rerender } = render(
    <FileBrowser adapter={files} paths={["a.txt"]} onOpen={onOpen} />,
  );
  await screen.findByRole("treeitem", { name: "a.txt" });
  expect(files.list).toHaveBeenCalledTimes(1);
  for (let i = 0; i < 5; i++)
    rerender(<FileBrowser adapter={files} paths={["a.txt"]} onOpen={onOpen} />);
  await act(async () => undefined);
  expect(files.list).toHaveBeenCalledTimes(1);
});
it("rebuilds a tree of fixed paths only when the paths change", async () => {
  const fixed: FileAdapter = { id: "fixed", read: vi.fn() };
  const onOpen = vi.fn();
  const { rerender } = render(
    <FileBrowser adapter={fixed} paths={["a.txt"]} onOpen={onOpen} />,
  );
  await screen.findByRole("treeitem", { name: "a.txt" });
  rerender(
    <FileBrowser adapter={fixed} paths={["a.txt", "b.txt"]} onOpen={onOpen} />,
  );
  expect(await screen.findByRole("treeitem", { name: "b.txt" })).toBeVisible();
});
it("waits for a slow folder check to finish before starting another", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let finish = () => {};
  const files = adapter();
  const list = files.list!;
  const stat = vi.fn(async (paths: string[]) => stamped(paths));
  files.stat = stat;
  files.list = vi.fn(async (directory) => {
    const listing = await list(directory);
    if (vi.mocked(files.list!).mock.calls.length > 1)
      await new Promise<void>((done) => (finish = done));
    return listing;
  });
  render(<FileBrowser adapter={files} paths={[]} onOpen={vi.fn()} />);
  await screen.findByRole("treeitem", { name: "a.txt" });
  await intervals(3);
  expect(stat).toHaveBeenCalledTimes(1);
  expect(files.list).toHaveBeenCalledTimes(2);
  await act(async () => finish());
  await intervals(1);
  expect(stat).toHaveBeenCalledTimes(2);
});
it("checks no folders while hidden", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const stat = vi.fn(async (paths: string[]) => stamped(paths));
  const files = adapter({ stat });
  render(
    <div hidden>
      <FileBrowser adapter={files} paths={[]} onOpen={vi.fn()} />
    </div>,
  );
  await waitFor(() => expect(files.list).toHaveBeenCalled());
  await intervals(3);
  expect(stat).not.toHaveBeenCalled();
});
it("expands folders on demand and opens files", async () => {
  const onOpen = vi.fn();
  const files = adapter();
  render(<FileBrowser adapter={files} paths={[]} onOpen={onOpen} />);
  const folder = await screen.findByRole("treeitem", { name: "code" });
  expect(files.list).toHaveBeenCalledTimes(1);
  await userEvent.click(folder);
  await userEvent.click(
    await screen.findByRole("treeitem", { name: "run.do" }),
  );
  expect(files.list).toHaveBeenCalledWith("code");
  expect(onOpen).toHaveBeenCalledWith({ path: "code/run.do" });
});

it("never opens a link and dims ignored folders", async () => {
  const onOpen = vi.fn();
  render(<FileBrowser adapter={adapter()} paths={[]} onOpen={onOpen} />);
  await userEvent.click(
    await screen.findByRole("treeitem", { name: /escape/ }),
  );
  expect(onOpen).not.toHaveBeenCalled();
  expect(screen.getByRole("treeitem", { name: /target/ })).toHaveAttribute(
    "data-ignored",
    "true",
  );
});

it("navigates by keyboard following the tree pattern", async () => {
  const onOpen = vi.fn();
  render(<FileBrowser adapter={adapter()} paths={[]} onOpen={onOpen} />);
  const first = await screen.findByRole("treeitem", { name: "code" });
  fireEvent.keyDown(first, { key: "ArrowRight" });
  const child = await screen.findByRole("treeitem", { name: "run.do" });
  fireEvent.keyDown(first, { key: "ArrowDown" });
  await waitFor(() => expect(child).toHaveAttribute("aria-selected", "true"));
  fireEvent.keyDown(child, { key: "Enter" });
  expect(onOpen).toHaveBeenCalledWith({ path: "code/run.do" });
});

it("renames through the adapter, reports the move and offers undo", async () => {
  dialogs.promptDialog.mockResolvedValue("b.txt");
  const onMoved = vi.fn();
  const files = adapter();
  render(
    <FileBrowser
      adapter={files}
      paths={[]}
      onOpen={vi.fn()}
      onMoved={onMoved}
    />,
  );
  fireEvent.contextMenu(await screen.findByRole("treeitem", { name: "a.txt" }));
  await userEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
  await waitFor(() =>
    expect(files.move).toHaveBeenCalledWith("a.txt", "b.txt"),
  );
  await waitFor(() => expect(onMoved).toHaveBeenCalledWith("a.txt", "b.txt"));
  const undo = dialogs.notify.mock.calls[0][2];
  expect(undo.label).toBe("Undo");
  undo.run();
  await waitFor(() =>
    expect(files.move).toHaveBeenCalledWith("b.txt", "a.txt"),
  );
});

it("asks before the Trash and does nothing when declined", async () => {
  dialogs.confirmDialog
    .mockResolvedValueOnce(false)
    .mockResolvedValueOnce(true);
  const files = adapter();
  const onTrashed = vi.fn();
  render(
    <FileBrowser
      adapter={files}
      paths={[]}
      onOpen={vi.fn()}
      onTrashed={onTrashed}
    />,
  );
  const row = await screen.findByRole("treeitem", { name: "a.txt" });
  fireEvent.contextMenu(row);
  await userEvent.click(
    screen.getByRole("menuitem", { name: "Move to Trash" }),
  );
  await waitFor(() => expect(dialogs.confirmDialog).toHaveBeenCalled());
  expect(files.trash).not.toHaveBeenCalled();
  fireEvent.contextMenu(row);
  await userEvent.click(
    screen.getByRole("menuitem", { name: "Move to Trash" }),
  );
  await waitFor(() => expect(files.trash).toHaveBeenCalledWith("a.txt"));
  await waitFor(() => expect(onTrashed).toHaveBeenCalledWith("a.txt"));
});

it("creates inside the selected folder and shows why a change failed", async () => {
  dialogs.promptDialog.mockResolvedValue("notes.md");
  const files = adapter({
    createFile: vi.fn(async () => {
      throw { message: "Stop the active research turn before changing files" };
    }),
  });
  render(<FileBrowser adapter={files} paths={[]} onOpen={vi.fn()} />);
  await userEvent.click(await screen.findByRole("treeitem", { name: "code" }));
  await userEvent.click(
    screen.getByRole("button", { name: "Add to this folder" }),
  );
  await userEvent.click(screen.getByRole("menuitem", { name: "New file" }));
  await waitFor(() =>
    expect(files.createFile).toHaveBeenCalledWith("code/notes.md"),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Stop the active research turn",
  );
});

it("offers no changes to a read-only adapter and builds its tree from paths", async () => {
  render(
    <FileBrowser
      adapter={{ id: "run", read: vi.fn() }}
      paths={["report.md", "artifacts/a.png"]}
      onOpen={vi.fn()}
    />,
  );
  await screen.findByRole("treeitem", { name: "report.md" });
  expect(
    screen.queryByRole("button", { name: "Add to this folder" }),
  ).toBeNull();
  fireEvent.contextMenu(screen.getByRole("treeitem", { name: "report.md" }));
  expect(screen.queryByRole("menu")).toBeNull();
});

it("filters by name and searches inside files on request", async () => {
  const search = vi.fn(async () => ({
    matches: [{ path: "code/run.do", line: 3, text: "regress wage" }],
    searchedFiles: 4,
    skippedFiles: 1,
    truncated: false,
  }));
  const onOpen = vi.fn();
  render(
    <FileBrowser
      adapter={adapter({ search })}
      paths={["code/run.do", "other.txt"]}
      onOpen={onOpen}
    />,
  );
  await screen.findByRole("treeitem", { name: "a.txt" });
  await userEvent.type(screen.getByLabelText("Find a file"), "wage");
  expect(screen.getByText("No file names match.")).toBeVisible();
  await userEvent.click(
    screen.getByRole("button", { name: "Search inside files" }),
  );
  expect(search).toHaveBeenCalledWith("wage");
  await userEvent.click(
    await screen.findByRole("button", { name: /run\.do:3/ }),
  );
  expect(onOpen).toHaveBeenCalledWith({ path: "code/run.do", line: 3 });
});

it("opens the first name match on Enter and returns to the tree", async () => {
  const onOpen = vi.fn();
  render(
    <FileBrowser adapter={adapter()} paths={["code/run.do"]} onOpen={onOpen} />,
  );
  await screen.findByRole("treeitem", { name: "a.txt" });
  await userEvent.type(screen.getByLabelText("Find a file"), "run{Enter}");
  expect(onOpen).toHaveBeenCalledWith({ path: "code/run.do" });
  expect(screen.getByLabelText("Find a file")).toHaveValue("");
  expect(
    await screen.findByRole("treeitem", { name: "run.do" }),
  ).toBeInTheDocument();
});

it("opens a row's menu from the keyboard and offers file actions", async () => {
  const onAskFile = vi.fn();
  render(
    <FileBrowser
      adapter={adapter()}
      paths={[]}
      onOpen={vi.fn()}
      onAskFile={onAskFile}
    />,
  );
  const file = await screen.findByRole("treeitem", { name: "a.txt" });
  fireEvent.keyDown(file, { key: "ContextMenu" });
  await userEvent.click(
    screen.getByRole("menuitem", { name: "Ask about this" }),
  );
  expect(onAskFile).toHaveBeenCalledWith("a.txt");
  fireEvent.contextMenu(screen.getByRole("treeitem", { name: "code" }));
  expect(screen.queryByRole("menuitem", { name: "Ask about this" })).toBeNull();
  expect(screen.getByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
});

it("says when a folder lists only part of what it holds", async () => {
  const listing = (directory: string, truncated: boolean) => ({
    directory,
    entries:
      directory === "" ? [entry("big", "folder")] : [entry("big/first.csv")],
    truncated,
  });
  const list = vi.fn(async (directory: string) =>
    listing(directory, directory === "big"),
  );
  render(
    <FileBrowser adapter={adapter({ list })} paths={[]} onOpen={vi.fn()} />,
  );
  expect(screen.queryByText("First 5,000 items")).toBeNull();
  await userEvent.click(await screen.findByText("big"));
  expect(await screen.findByText("First 5,000 items")).toBeInTheDocument();
  expect(screen.queryByText(/Use the filter/)).toBeNull();
});
