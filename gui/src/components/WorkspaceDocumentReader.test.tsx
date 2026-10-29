const pdfProps = vi.hoisted(() => ({
  current: null as null | {
    initialPage?: number;
    position?: { page: number };
    onPosition?: (value: { page: number }) => void;
  },
}));
vi.mock("./file-workspace/PdfReader", () => ({
  default: (props: {
    fallback: import("react").ReactNode;
    position?: { page: number };
  }) => {
    pdfProps.current = props;
    return <>{props.fallback}</>;
  },
}));
const paperSearch = vi.hoisted(() => vi.fn());
vi.mock("../lib/workbench/researchClient", () => ({
  researchClient: { paperSearch },
}));
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceDocumentReader from "./WorkspaceDocumentReader";
import type { DocumentView } from "../lib/projectClient";
const read = vi.hoisted(() => vi.fn());
vi.mock("../lib/projectClient", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/projectClient")>()),
  projectClient: { read },
}));
vi.mock("./ReportViewer", () => ({
  default: ({ markdown }: { markdown: string }) => <div>{markdown}</div>,
}));
const view = (text: string): DocumentView => ({
  title: "Paper",
  revision: {
    id: "revision",
    paperId: "paper",
    inputKind: "text",
    contentHash: "abcdef",
    entrypoint: "paper.txt",
    textReference: null,
    dependencyManifest: {},
    compiledArtifactId: null,
    extraction: { status: "complete" },
    captureComplete: true,
    capturedAt: "now",
  },
  start: 0,
  end: text.length,
  totalBytes: text.length,
  pageCount: null,
  page: null,
  imageUrl: null,
  text,
});
beforeEach(() => {
  localStorage.clear();
  read.mockReset();
});
afterEach(cleanup);
it("reading source and math needs no conversation action", async () => {
  read.mockResolvedValue(view("Exact original passage"));
  const action = vi.fn();
  render(
    <WorkspaceDocumentReader
      workspaceId="workspace"
      revisionId="revision"
      annotations={[]}
      onSelection={action}
      onError={vi.fn()}
    />,
  );
  await screen.findByText("Exact original passage");
  fireEvent.click(screen.getByRole("button", { name: "Read with math" }));
  await screen.findByText("Use Text / source to attach an exact selection.");
  expect(action).not.toHaveBeenCalled();
});
it("attaches the immutable selection when opening an annotation", async () => {
  read.mockResolvedValue(view("Original passage"));
  const action = vi.fn();
  const selection = {
    revisionId: "revision",
    revisionHash: "abcdef",
    start: 0,
    end: 8,
    quote: "Original",
    page: null,
    region: null,
  };
  render(
    <WorkspaceDocumentReader
      workspaceId="workspace"
      revisionId="revision"
      initialSelection={selection}
      annotations={[]}
      onSelection={action}
      onError={vi.fn()}
    />,
  );
  await screen.findByText("Original passage");
  fireEvent.click(screen.getByRole("button", { name: "Create action item" }));
  expect(action).toHaveBeenCalledWith(selection, "task");
});
it("opens an annotation at its passage instead of the saved scroll position", async () => {
  localStorage.setItem(
    "pipeline.reader.workspace.revision",
    JSON.stringify({ start: 0, mode: "source", scroll: 120 }),
  );
  read.mockResolvedValue(view("Original passage"));
  render(
    <WorkspaceDocumentReader
      workspaceId="workspace"
      revisionId="revision"
      initialSelection={{
        revisionId: "revision",
        revisionHash: "abcdef",
        start: 0,
        end: 8,
        quote: "Original",
        page: null,
        region: null,
      }}
      annotations={[]}
      onSelection={vi.fn()}
      onError={vi.fn()}
    />,
  );
  const scroller = (await screen.findByText("Original passage")).closest(
    ".overflow-auto",
  )!;
  await act(
    () => new Promise((resolve) => requestAnimationFrame(() => resolve(null))),
  );
  // The saved offset (120) belonged to an earlier reading of this segment.
  expect(scroller.scrollTop).toBe(0);
  expect(layout()).toMatchObject({ start: 0, scroll: 0 });
});
it("ignores a stale reader request after switching to a PDF page", async () => {
  let resolveOld!: (value: DocumentView) => void;
  read.mockResolvedValueOnce({
    ...view("first"),
    revision: { ...view("").revision, inputKind: "pdf" },
  });
  const error = vi.fn();
  render(
    <WorkspaceDocumentReader
      workspaceId="workspace"
      revisionId="revision"
      annotations={[]}
      onSelection={vi.fn()}
      onError={error}
    />,
  );
  await screen.findByText("first");
  read.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveOld = resolve;
      }),
  );
  fireEvent.click(screen.getByRole("button", { name: "PDF page" }));
  fireEvent.click(
    await screen.findByRole("button", { name: "Open rendered page images" }),
  );
  read.mockResolvedValueOnce({
    ...view("latest"),
    revision: { ...view("").revision, inputKind: "pdf" },
    page: 2,
    imageUrl: "data:image/jpeg;base64,AA==",
  });
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByAltText("Paper, page 2");
  resolveOld({ ...view("stale"), title: "Stale paper" });
  await waitFor(() => expect(screen.queryByText("Stale paper")).toBeNull());
  expect(error).not.toHaveBeenCalled();
});

it("reads Markdown with highlighting without crashing its parent", async () => {
  const markdown = view("# Research note\nA stable **claim** and $α=2$.");
  markdown.revision.entrypoint = "note.md";
  read.mockResolvedValue(markdown);
  const error = vi.fn();
  render(
    <WorkspaceDocumentReader
      workspaceId="workspace"
      revisionId="revision"
      annotations={[]}
      onSelection={vi.fn()}
      onError={error}
    />,
  );
  await screen.findByRole("heading", { name: "Paper" });
  await waitFor(() =>
    expect(document.querySelector("pre")?.textContent).toContain(
      "Research note",
    ),
  );
  expect(error).not.toHaveBeenCalled();
});

const pdfView = (page: number | null = null): DocumentView => ({
  ...view("PDF text"),
  revision: { ...view("").revision, inputKind: "pdf" },
  pageCount: 4,
  page,
  imageUrl: page ? "data:image/jpeg;base64,AA==" : null,
});
const layout = () =>
  JSON.parse(localStorage.getItem("pipeline.reader.workspace.revision")!);

it("follows the interactive PDF's own page and leaves page controls to it", async () => {
  read.mockResolvedValue(pdfView());
  render(
    <WorkspaceDocumentReader
      workspaceId="workspace"
      revisionId="revision"
      annotations={[]}
      onSelection={vi.fn()}
      onError={vi.fn()}
    />,
  );
  await screen.findByText("PDF text");
  fireEvent.click(screen.getByRole("button", { name: "PDF page" }));
  await screen.findByRole("button", { name: "Open rendered page images" });
  expect(screen.queryByLabelText("PDF page")).toBeNull();
  expect(pdfProps.current?.initialPage).toBeUndefined();
  expect(pdfProps.current?.position).toEqual({ page: 1 });
  act(() => pdfProps.current?.onPosition?.({ page: 3 }));
  expect(pdfProps.current?.position).toEqual({ page: 3 });
  expect(layout()).toMatchObject({ page: 3, mode: "page" });
});

it("sanitizes saved and typed page numbers instead of crashing", async () => {
  localStorage.setItem(
    "pipeline.reader.workspace.revision",
    JSON.stringify({ page: 2.5, mode: "page" }),
  );
  read.mockImplementation(async (_w, _r, _s, page: number | null) =>
    pdfView(page),
  );
  const error = vi.fn();
  render(
    <WorkspaceDocumentReader
      workspaceId="workspace"
      revisionId="revision"
      annotations={[]}
      onSelection={vi.fn()}
      onError={error}
    />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Open rendered page images" }),
  );
  expect(pdfProps.current?.position).toEqual({ page: 2 });
  const box = await screen.findByLabelText("PDF page");
  expect(box).toHaveValue(2);
  fireEvent.change(box, { target: { value: "3.7" } });
  expect(read).not.toHaveBeenLastCalledWith("workspace", "revision", 0, 3);
  fireEvent.keyDown(box, { key: "Enter" });
  await screen.findByAltText("Paper, page 3");
  fireEvent.change(box, { target: { value: "9" } });
  fireEvent.blur(box);
  await screen.findByAltText("Paper, page 4");
  expect(screen.getByRole("button", { name: "Next" })).toBeDisabled();
  expect(paperSearch).not.toHaveBeenCalled();
  expect(error).not.toHaveBeenCalled();
});

it("restores the saved scroll only for its own segment and opens a new segment at the top", async () => {
  localStorage.setItem(
    "pipeline.reader.workspace.revision",
    JSON.stringify({ start: 0, mode: "source", scroll: 120 }),
  );
  read
    .mockResolvedValueOnce({
      ...view("First segment"),
      end: 13,
      totalBytes: 30,
    })
    .mockResolvedValueOnce({
      ...view("Second segment"),
      start: 13,
      end: 27,
      totalBytes: 30,
    });
  render(
    <WorkspaceDocumentReader
      workspaceId="workspace"
      revisionId="revision"
      annotations={[]}
      onSelection={vi.fn()}
      onError={vi.fn()}
    />,
  );
  const scroller = (await screen.findByText("First segment")).closest(
    ".overflow-auto",
  )!;
  await waitFor(() => expect(scroller.scrollTop).toBe(120));
  scroller.scrollTop = 900;
  fireEvent.scroll(scroller);
  await waitFor(() => expect(layout().scroll).toBe(900));
  fireEvent.click(screen.getByRole("button", { name: "Next text segment" }));
  await screen.findByText("Second segment");
  await waitFor(() => expect(scroller.scrollTop).toBe(0));
  expect(layout()).toMatchObject({ start: 13, scroll: 0 });
});
