import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import FileWorkspace from "./FileWorkspace";
import type { FileAdapter, FilePreview } from "../../lib/fileWorkspaceClient";

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
const tick = () =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(2100);
  });
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
});
afterEach(() => vi.useRealTimers());

it("reloads a clean file that changed on disk", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let stamp = "stamp-1";
  const read = vi.fn(async (path: string) =>
    file(
      path,
      stamp === "stamp-1"
        ? {}
        : { hash: "second", text: "From the assistant", stamp },
    ),
  );
  render(
    <FileWorkspace
      adapter={polled(read, () => stamp)}
      paths={[]}
      initial={{ path: "a.py" }}
    />,
  );
  await screen.findByLabelText("Source editor: a.py");
  stamp = "stamp-2";
  await tick();
  await waitFor(() =>
    expect(screen.getByLabelText("Source editor: a.py")).toHaveValue(
      "From the assistant",
    ),
  );
});

it("keeps a draft and says so when its file changes on disk", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let stamp = "stamp-1";
  const read = vi.fn(async (path: string) =>
    file(path, stamp === "stamp-1" ? {} : { hash: "second", stamp }),
  );
  render(
    <FileWorkspace
      adapter={polled(read, () => stamp)}
      paths={[]}
      initial={{ path: "a.py" }}
    />,
  );
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {
    target: { value: "My draft" },
  });
  stamp = "stamp-2";
  await tick();
  expect(await screen.findByText(/The file changed on disk/)).toBeVisible();
  expect(screen.getByLabelText("Source editor: a.py")).toHaveValue("My draft");
});

it("adopts a new stamp silently when the bytes are the same", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let stamp = "stamp-1";
  const read = vi.fn(async (path: string) => file(path, { stamp }));
  render(
    <FileWorkspace
      adapter={polled(read, () => stamp)}
      paths={[]}
      initial={{ path: "a.py" }}
    />,
  );
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {
    target: { value: "My draft" },
  });
  stamp = "stamp-2";
  await tick();
  expect(screen.queryByText(/The file changed on disk/)).toBeNull();
});

it("says when an open file was removed and keeps its draft", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  let gone = false;
  const read = vi.fn(async (path: string) => file(path));
  render(
    <FileWorkspace
      adapter={polled(read, () => (gone ? null : "stamp-1"))}
      paths={[]}
      initial={{ path: "a.py" }}
    />,
  );
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {
    target: { value: "My draft" },
  });
  gone = true;
  await tick();
  expect(await screen.findByText(/moved or deleted/)).toBeVisible();
  expect(screen.getByLabelText("Source editor: a.py")).toHaveValue("My draft");
});

it("carries tabs and drafts across a rename", async () => {
  dialogs.promptDialog.mockResolvedValue("b.py");
  const read = vi.fn(async (path: string) => file(path));
  const adapter: FileAdapter = {
    id: "moves",
    read,
    save: vi.fn(async () => "saved"),
    list: vi.fn(async (directory) => ({
      directory,
      truncated: false,
      entries: [
        {
          name: "a.py",
          path: "a.py",
          kind: "file" as const,
          size: 1,
          modifiedMs: 0,
          ignored: false,
          executable: false,
        },
      ],
    })),
    move: vi.fn(async () => undefined),
  };
  render(
    <FileWorkspace adapter={adapter} paths={[]} initial={{ path: "a.py" }} />,
  );
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {
    target: { value: "My draft" },
  });
  fireEvent.contextMenu(await screen.findByRole("treeitem", { name: "a.py" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
  expect(await screen.findByLabelText("Source editor: b.py")).toHaveValue(
    "My draft",
  );
  expect(screen.getByRole("tab", { name: "b.py •" })).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(adapter.save).toHaveBeenCalledWith(
      expect.objectContaining({ path: "b.py", hash: "first" }),
      "My draft",
    ),
  );
  await waitFor(() =>
    expect(localStorage.getItem("pipeline.files.draft.moves.a.py")).toBeNull(),
  );
});

it("releases a viewer's stream when its tab closes", async () => {
  const release = vi.fn();
  const read = vi.fn(async (path: string) =>
    file(path, {
      kind: "image",
      mime: "image/png",
      text: null,
      editable: false,
      streamToken: "token-1",
    }),
  );
  const { unmount } = render(
    <FileWorkspace
      adapter={{ id: "stream", read, release }}
      paths={[]}
      initial={{ path: "a.png" }}
    />,
  );
  expect(await screen.findByRole("img", { name: "a.png" })).toHaveAttribute(
    "src",
    "stream://token-1",
  );
  unmount();
  expect(release).toHaveBeenCalledWith("token-1");
});

it("shows editable CSV files as tables, with source one click away", async () => {
  const read = vi.fn(async (path: string) =>
    file(path, { text: "name,value\na,1\n" }),
  );
  render(
    <FileWorkspace
      adapter={{ id: "csv", read, save: vi.fn() }}
      paths={[]}
      initial={{ path: "data.csv" }}
    />,
  );
  expect(await screen.findByRole("table")).toHaveTextContent("value");
  fireEvent.click(screen.getByRole("button", { name: "Source" }));
  expect(await screen.findByLabelText("Source editor: data.csv")).toHaveValue(
    "name,value\na,1\n",
  );
});

const liveCsv = (path: string) =>
  file(path, {
    kind: "table",
    text: null,
    mime: "application/octet-stream",
    bytes: 3 * 1024 * 1024 * 1024,
  });

it("opens a project CSV as a table of its first rows, with no source view", async () => {
  const read = vi.fn(async (path: string) => liveCsv(path));
  const table = vi.fn(async () => ({
    columns: ["name", "value"],
    rows: [["a", "1"]],
    start: 0,
    pageSize: 100,
    totalRows: 1,
    sheet: null,
    sheets: [],
    preview: {
      nextOffset: 20,
      endReached: false,
      columnCount: 2,
      clipped: [],
      approximate: false,
      stamp: "1",
    },
  }));
  render(
    <FileWorkspace
      adapter={{ id: "csv", read, save: vi.fn(), table, external: vi.fn() }}
      paths={[]}
      initial={{ path: "panel.csv" }}
    />,
  );
  expect(await screen.findByRole("table")).toHaveTextContent("value");
  expect(table).toHaveBeenCalledWith("panel.csv", null, 0);
  expect(screen.getByText("First 1 row · 2 columns")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Source" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "More file actions" }));
  expect(
    await screen.findByRole("menuitem", { name: "Open externally" }),
  ).toBeInTheDocument();
});

it("says plainly when a table cannot be previewed in this view", async () => {
  const read = vi.fn(async (path: string) => liveCsv(path));
  render(
    <FileWorkspace
      adapter={{ id: "captured", read }}
      paths={[]}
      initial={{ path: "panel.csv" }}
    />,
  );
  expect(
    await screen.findByText("This table cannot be previewed here"),
  ).toBeInTheDocument();
  expect(screen.getByText("Table file · 3.0 GB")).toBeInTheDocument();
  expect(screen.queryByText("Pipeline cannot show this type")).toBeNull();
});

it("shows a data file's size and actions without reading it", async () => {
  const table = vi.fn();
  const read = vi.fn(async (path: string) =>
    file(path, {
      kind: "data",
      text: null,
      mime: "application/octet-stream",
      bytes: 3 * 1024 * 1024 * 1024,
    }),
  );
  render(
    <FileWorkspace
      adapter={{ id: "data", read, save: vi.fn(), table, external: vi.fn() }}
      paths={[]}
      initial={{ path: "panel.dta" }}
    />,
  );
  expect(
    await screen.findByText("Data files are not shown here"),
  ).toBeInTheDocument();
  expect(screen.getByText("Data file · 3.0 GB")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Open externally" })).toBeEnabled();
  expect(screen.queryByRole("table")).toBeNull();
  expect(table).not.toHaveBeenCalled();
});

it("loads the rest of a large text file on request", async () => {
  const fetchMock = vi.fn(
    async () => new Response("second part", { status: 206 }),
  );
  vi.stubGlobal("fetch", fetchMock);
  const read = vi.fn(async (path: string) =>
    file(path, {
      text: "first part ",
      bytes: 22,
      truncated: true,
      editable: false,
      streamToken: "token-2",
    }),
  );
  render(
    <FileWorkspace
      adapter={{ id: "large", read }}
      paths={[]}
      initial={{ path: "run.log" }}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Load more" }));
  await waitFor(() =>
    expect(screen.getByLabelText("Source editor: run.log")).toHaveValue(
      "first part second part",
    ),
  );
  vi.unstubAllGlobals();
});

it("names what it cannot show and offers the system's viewer", async () => {
  const external = vi.fn(async () => undefined);
  const read = vi.fn(async (path: string) =>
    file(path, {
      kind: "binary",
      mime: "application/octet-stream",
      text: null,
      editable: false,
      bytes: 2048,
    }),
  );
  render(
    <FileWorkspace
      adapter={{ id: "binary", read, external }}
      paths={[]}
      initial={{ path: "a.heic" }}
    />,
  );
  expect(await screen.findByText(/Binary file · 2.0 KB/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Open externally" }));
  expect(external).toHaveBeenCalledWith("a.heic");
});
