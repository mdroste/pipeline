// What a tab does when its file changes, leaves, or comes back underneath it.
import {
  act,
  fireEvent,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import FileWorkspace from "./FileWorkspace";
import { useFileBuffers } from "./useFileBuffers";
import type {
  FileAdapter,
  FileListEntry,
  FilePreview,
} from "../../lib/fileWorkspaceClient";

vi.mock("./SourceEditor", () => ({
  default: ({
    value,
    onChange,
    label,
  }: {
    value: string;
    onChange: (s: string) => void;
    label: string;
  }) => (
    <textarea
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));
const dialogs = vi.hoisted(() => ({
  confirmDialog: vi.fn(),
  promptDialog: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../DialogService", () => dialogs);
vi.mock("../../lib/fileDrop", () => ({
  onFileDrop: async () => () => undefined,
}));
vi.mock("../ReportViewer", () => ({
  default: ({ markdown }: { markdown: string }) => <div>{markdown}</div>,
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (token: string) => `stream://${token}`,
  invoke: vi.fn(),
}));

const file = (
  path: string,
  overrides: Partial<FilePreview> = {},
): FilePreview => ({
  path,
  hash: "first",
  text: "Original",
  bytes: 8,
  base64: null,
  mime: "text/plain",
  editable: true,
  truncated: false,
  externalPath: null,
  stamp: "stamp-1",
  ...overrides,
});
const changed = file("a.py", {
  hash: "second",
  text: "From disk",
  stamp: "stamp-2",
});
const entry = (path: string, kind: FileListEntry["kind"] = "file") => ({
  name: path.split("/").pop()!,
  path,
  kind,
  size: 1,
  modifiedMs: 0,
  ignored: false,
  executable: false,
});
function polled(read: FileAdapter["read"], stamp: () => string | null) {
  return {
    id: "live",
    read,
    save: vi.fn(async () => "saved"),
    release: vi.fn(),
    stat: vi.fn(async (paths: string[]) =>
      paths.map((path) => ({
        path,
        kind: "file" as const,
        stamp: stamp(),
        size: 1,
        modifiedMs: 1,
      })),
    ),
  } satisfies FileAdapter;
}
function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
}
const tick = () =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(2100);
  });
const restore = (id: string, tabs: string[], active: string) =>
  localStorage.setItem(
    `pipeline.files.layout.${id}`,
    JSON.stringify({ tabs, active: { path: active }, split: null }),
  );
const editor = (path: string) =>
  screen.findByLabelText(`Source editor: ${path}`);

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
});
afterEach(() => vi.useRealTimers());

it("keeps text typed while the poller reads a changed file", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let stamp = "stamp-1";
  const slow = held<FilePreview>();
  const read = vi
    .fn<FileAdapter["read"]>()
    .mockResolvedValueOnce(file("a.py"))
    .mockReturnValue(slow.promise);
  render(
    <FileWorkspace
      adapter={polled(read, () => stamp)}
      paths={[]}
      initial={{ path: "a.py" }}
    />,
  );
  await editor("a.py");
  stamp = "stamp-2";
  await tick();
  expect(read).toHaveBeenCalledTimes(2);
  fireEvent.change(await editor("a.py"), { target: { value: "My typing" } });
  await act(async () => slow.resolve(changed));
  expect(await screen.findByText(/changed on disk/)).toBeVisible();
  expect(await editor("a.py")).toHaveValue("My typing");
});

it("keeps text typed while a clean file is reloading", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let stamp = "stamp-1";
  const slow = held<FilePreview>();
  const read = vi
    .fn<FileAdapter["read"]>()
    .mockResolvedValueOnce(file("a.py"))
    .mockResolvedValueOnce(changed)
    .mockReturnValueOnce(slow.promise)
    .mockResolvedValue(changed);
  render(
    <FileWorkspace
      adapter={polled(read, () => stamp)}
      paths={[]}
      initial={{ path: "a.py" }}
    />,
  );
  await editor("a.py");
  stamp = "stamp-2";
  await tick();
  expect(read).toHaveBeenCalledTimes(3);
  fireEvent.change(await editor("a.py"), { target: { value: "My typing" } });
  await act(async () => slow.resolve(changed));
  expect(await editor("a.py")).toHaveValue("My typing");
  await tick();
  expect(await screen.findByText(/changed on disk/)).toBeVisible();
  expect(await editor("a.py")).toHaveValue("My typing");
});

it("replaces a draft only for a Reload the researcher confirmed", async () => {
  const adapter: FileAdapter = {
    id: "reload",
    read: vi
      .fn<FileAdapter["read"]>()
      .mockResolvedValueOnce(file("a.py"))
      .mockResolvedValue(changed),
  };
  const { result } = renderHook(() =>
    useFileBuffers({ adapter, onConflict: vi.fn(), setError: vi.fn() }),
  );
  await act(() => result.current.load("a.py"));
  act(() =>
    result.current.update("a.py", (old) => ({ ...old, draft: "My typing" })),
  );
  await act(() => result.current.load("a.py", true));
  expect(result.current.buffers["a.py"].draft).toBe("My typing");
  await act(() => result.current.load("a.py", "discard"));
  expect(result.current.buffers["a.py"].draft).toBe("From disk");
});

it("reloads a file that disappeared and came back changed", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let stamp: string | null = "stamp-1";
  const read = vi.fn(async (path: string) =>
    stamp === "stamp-1" ? file(path) : changed,
  );
  render(
    <FileWorkspace
      adapter={polled(read, () => stamp)}
      paths={[]}
      initial={{ path: "a.py" }}
    />,
  );
  await editor("a.py");
  stamp = null;
  await tick();
  expect(await screen.findByText(/moved or deleted/)).toBeVisible();
  stamp = "stamp-2";
  await tick();
  await waitFor(async () =>
    expect(await editor("a.py")).toHaveValue("From disk"),
  );
  expect(screen.queryByText(/moved or deleted/)).toBeNull();
});

it("notices a file restored with the stamp it left with", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let stamp: string | null = "stamp-1";
  const read = vi.fn(async (path: string) => file(path));
  render(
    <FileWorkspace
      adapter={polled(read, () => stamp)}
      paths={[]}
      initial={{ path: "a.py" }}
    />,
  );
  fireEvent.change(await editor("a.py"), { target: { value: "My draft" } });
  stamp = null;
  await tick();
  expect(await screen.findByText(/moved or deleted/)).toBeVisible();
  stamp = "stamp-1";
  await tick();
  await waitFor(() =>
    expect(screen.queryByText(/moved or deleted/)).toBeNull(),
  );
  expect(await editor("a.py")).toHaveValue("My draft");
});

it("closes a tab whose file is gone, with a readable message", async () => {
  restore("gone", ["a.py"], "a.py");
  localStorage.setItem(
    "pipeline.files.draft.gone.a.py",
    JSON.stringify({
      file: { path: "a.py", hash: "h", text: "Old" },
      draft: "Mine",
    }),
  );
  const read = vi.fn(async () => {
    throw { message: "Missing file: a.py", code: "file_missing" };
  });
  render(<FileWorkspace adapter={{ id: "gone", read }} paths={[]} />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Missing file: a.py",
  );
  expect(screen.queryByText(/Opening/)).toBeNull();
  expect(screen.queryByText(/object Object/)).toBeNull();
  expect(screen.queryAllByRole("tab")).toEqual([]);
  expect(localStorage.getItem("pipeline.files.draft.gone.a.py")).toContain(
    "Mine",
  );
});

it("stops saying Opening when a file can't be read", async () => {
  restore("locked", ["a.py"], "a.py");
  const read = vi.fn(async () => {
    throw { message: "Permission denied", code: "storage" };
  });
  render(<FileWorkspace adapter={{ id: "locked", read }} paths={[]} />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Permission denied",
  );
  expect(await screen.findByText("a.py could not be opened.")).toBeVisible();
  expect(screen.queryByText(/Opening/)).toBeNull();
  expect(screen.getByRole("tab", { name: "a.py" })).toBeVisible();
});

it("closes every tab in a folder sent to the Trash", async () => {
  dialogs.confirmDialog.mockResolvedValue(true);
  restore("trash", ["d/one.txt", "d/two.txt", "d/unread.txt"], "d/one.txt");
  const adapter: FileAdapter = {
    id: "trash",
    read: vi.fn(async (path: string) => file(path)),
    list: vi.fn(async (directory) => ({
      directory,
      truncated: false,
      entries: directory ? [] : [entry("d", "folder")],
    })),
    trash: vi.fn(async () => undefined),
  };
  render(<FileWorkspace adapter={adapter} paths={[]} />);
  await editor("d/one.txt");
  fireEvent.click(screen.getByRole("tab", { name: "two.txt" }));
  await editor("d/two.txt");
  fireEvent.contextMenu(await screen.findByRole("treeitem", { name: "d" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Move to Trash" }));
  await waitFor(() => expect(adapter.trash).toHaveBeenCalledWith("d"));
  await waitFor(() => expect(screen.queryAllByRole("tab")).toEqual([]));
  expect(screen.queryByRole("alert")).toBeNull();
});

it("brings the draft of an unread tab along with a rename", async () => {
  dialogs.promptDialog.mockResolvedValue("b.py");
  restore("moves", ["a.py", "other.py"], "other.py");
  localStorage.setItem(
    "pipeline.files.draft.moves.a.py",
    JSON.stringify({
      file: { path: "a.py", hash: "first", text: "Original" },
      draft: "My draft",
    }),
  );
  const read = vi.fn(async (path: string) => file(path));
  const adapter: FileAdapter = {
    id: "moves",
    read,
    list: vi.fn(async (directory) => ({
      directory,
      truncated: false,
      entries: [entry("a.py"), entry("other.py")],
    })),
    move: vi.fn(async () => undefined),
  };
  render(<FileWorkspace adapter={adapter} paths={[]} />);
  await editor("other.py");
  expect(read).not.toHaveBeenCalledWith("a.py");
  fireEvent.contextMenu(await screen.findByRole("treeitem", { name: "a.py" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
  fireEvent.click(await screen.findByRole("tab", { name: "b.py" }));
  expect(await editor("b.py")).toHaveValue("My draft");
  expect(localStorage.getItem("pipeline.files.draft.moves.a.py")).toBeNull();
});

it("moves unread drafts with a folder and leaves other drafts alone", () => {
  const seed = (key: string, path: string) =>
    localStorage.setItem(
      key,
      JSON.stringify({
        file: { path, hash: "h", text: "Old" },
        draft: `Draft of ${path}`,
      }),
    );
  seed("pipeline.files.draft.a.old/one.tex", "old/one.tex");
  seed("pipeline.files.draft.a.old/deep/two.tex", "old/deep/two.tex");
  seed("pipeline.files.draft.a.older/three.tex", "older/three.tex");
  seed("pipeline.files.draft.a.b.old/four.tex", "old/four.tex");
  localStorage.setItem("pipeline.files.draft.a.old/broken.tex", "not json");
  const { result } = renderHook(() =>
    useFileBuffers({
      adapter: { id: "a", read: vi.fn() },
      onConflict: vi.fn(),
      setError: vi.fn(),
    }),
  );
  act(() => void result.current.remap("old", "new"));
  const stored = (key: string) =>
    JSON.parse(localStorage.getItem(`pipeline.files.draft.${key}`) ?? "null");
  expect(stored("a.new/one.tex")).toMatchObject({
    file: { path: "new/one.tex" },
    draft: "Draft of old/one.tex",
  });
  expect(stored("a.new/deep/two.tex").file.path).toBe("new/deep/two.tex");
  expect(stored("a.old/one.tex")).toBeNull();
  expect(stored("a.older/three.tex").file.path).toBe("older/three.tex");
  expect(stored("a.b.old/four.tex").file.path).toBe("old/four.tex");
});

it("keeps Load more text with the file and version it came from", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response("tail of a", { status: 206 })),
  );
  restore("live", ["a.log", "b.log"], "a.log");
  let stamp = "stamp-1";
  const large = (path: string, text: string, overrides = {}) =>
    file(path, {
      text,
      bytes: 23,
      truncated: true,
      editable: false,
      streamToken: `token-${path}`,
      ...overrides,
    });
  const read = vi.fn(async (path: string) =>
    stamp === "stamp-1"
      ? large(path, `head of ${path} `)
      : large(path, "new head ", { hash: "second", stamp }),
  );
  render(
    <FileWorkspace adapter={polled(read, () => stamp)} paths={["a.log"]} />,
  );
  await editor("a.log");
  fireEvent.click(screen.getByRole("button", { name: "Load more" }));
  await waitFor(async () =>
    expect(await editor("a.log")).toHaveValue("head of a.log tail of a"),
  );
  fireEvent.click(screen.getByRole("tab", { name: "b.log" }));
  expect(await editor("b.log")).toHaveValue("head of b.log ");
  fireEvent.click(screen.getByRole("tab", { name: "a.log" }));
  await waitFor(async () =>
    expect(await editor("a.log")).toHaveValue("head of a.log "),
  );
  fireEvent.click(screen.getByRole("button", { name: "Load more" }));
  await waitFor(async () =>
    expect(await editor("a.log")).toHaveValue("head of a.log tail of a"),
  );
  stamp = "stamp-2";
  await tick();
  await waitFor(async () =>
    expect(await editor("a.log")).toHaveValue("new head "),
  );
  vi.unstubAllGlobals();
});
