vi.mock("./file-workspace/PdfReader", () => ({
  default: ({ fallback }: { fallback: import("react").ReactNode }) => (
    <>{fallback}</>
  ),
}));
import {
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
