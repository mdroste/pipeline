// The file pane with the real editor. The other file tests replace it with a
// textarea, which never reports a change the program made.
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { EditorView } from "@codemirror/view";
import FileWorkspace from "./FileWorkspace";
import type { FilePreview } from "../../lib/fileWorkspaceClient";

vi.mock("../DialogService", () => ({
  confirmDialog: vi.fn(),
  promptDialog: vi.fn(),
  notify: vi.fn(),
}));
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

beforeAll(() => {
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
});
afterEach(() => vi.unstubAllGlobals());

const large: FilePreview = {
  path: "run.log",
  hash: "first",
  text: "first part ",
  bytes: 22,
  base64: null,
  mime: "text/plain",
  editable: false,
  truncated: true,
  externalPath: null,
  stamp: "stamp-1",
  streamToken: "token",
};
const settle = () =>
  act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 50));
  });

async function loadOnce() {
  const fetchMock = vi.fn(
    async () => new Response("second part", { status: 206 }),
  );
  vi.stubGlobal("fetch", fetchMock);
  render(
    <FileWorkspace
      adapter={{ id: "large", read: async () => large }}
      paths={[]}
      initial={{ path: "run.log" }}
    />,
  );
  const button = await screen.findByRole("button", { name: "Load more" });
  const view = EditorView.findFromDOM(
    screen.getByRole("textbox", { name: "Source editor: run.log" }),
  )!;
  fireEvent.click(button);
  await settle();
  return { fetchMock, view };
}

it("adds one chunk to a large file and stays that length", async () => {
  const { fetchMock, view } = await loadOnce();
  expect(view.state.sliceDoc()).toBe("first part second part");
  await settle();
  expect(view.state.sliceDoc()).toBe("first part second part");
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("fetches a range once however quickly Load more is pressed", async () => {
  let finish: (response: Response) => void = () => undefined;
  const fetchMock = vi.fn(
    () => new Promise<Response>((resolve) => (finish = resolve)),
  );
  vi.stubGlobal("fetch", fetchMock);
  render(
    <FileWorkspace
      adapter={{ id: "large", read: async () => large }}
      paths={[]}
      initial={{ path: "run.log" }}
    />,
  );
  const button = await screen.findByRole("button", { name: "Load more" });
  fireEvent.click(button);
  fireEvent.click(button);
  await settle();
  expect(fetchMock).toHaveBeenCalledTimes(1);
  await act(async () => finish(new Response("second part", { status: 206 })));
  await settle();
  const view = EditorView.findFromDOM(
    screen.getByRole("textbox", { name: "Source editor: run.log" }),
  )!;
  expect(view.state.sliceDoc()).toBe("first part second part");
});

it("continues from the bytes it has read, not the length of decoded text", async () => {
  // Latin-1: each é is one byte on disk and three once decoded and re-encoded.
  const bytes = Uint8Array.from([0xe9, 0x20, 0xe9, 0x20, 0x41, 0x42]);
  const ranges: (string | null)[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (_url: string, init: RequestInit) => {
      const range = new Headers(init.headers).get("Range");
      ranges.push(range);
      const [start, end] = range!.slice(6).split("-").map(Number);
      return new Response(bytes.slice(start, end + 1), { status: 206 });
    }),
  );
  const head = "\ufffd ";
  const file = {
    ...large,
    text: head,
    bytes: bytes.length,
    textBytes: 2,
    approximateText: true,
  };
  render(
    <FileWorkspace
      adapter={{ id: "latin", read: async () => file }}
      paths={[]}
      initial={{ path: "run.log" }}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Load more" }));
  await settle();
  expect(ranges).toEqual(["bytes=2-5"]);
  const view = EditorView.findFromDOM(
    screen.getByRole("textbox", { name: "Source editor: run.log" }),
  )!;
  expect(view.state.sliceDoc()).toBe("\ufffd \ufffd AB");
  expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
});

it("keeps no draft of a file that can't be edited", async () => {
  const { view } = await loadOnce();
  act(() => view.dispatch({ changes: { from: 0, insert: "x" } }));
  await settle();
  expect(view.state.sliceDoc()).toBe("xfirst part second part");
});
