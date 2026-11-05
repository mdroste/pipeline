import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ImageViewer from "./ImageViewer";
import NotebookViewer, { parseNotebook } from "./NotebookViewer";
import TableViewer from "./TableViewer";
import HtmlViewer from "./HtmlViewer";
import MediaViewer from "./MediaViewer";
import type {
  FileAdapter,
  FileTable,
  TablePreview,
} from "../../../lib/fileWorkspaceClient";

vi.mock("../../ReportViewer", () => ({
  default: ({ markdown }: { markdown: string }) => <div>{markdown}</div>,
}));

it("zooms from fit and reports when an image cannot be decoded", () => {
  render(<ImageViewer src="x" alt="plot.png" bytes={2048} />);
  const image = screen.getByRole("img", { name: "plot.png" });
  Object.defineProperty(image, "naturalWidth", { value: 800 });
  Object.defineProperty(image, "naturalHeight", { value: 600 });
  fireEvent.load(image);
  expect(screen.getByText(/800 × 600 · 2\.0 KB/)).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Actual size" }));
  expect(image).toHaveStyle({ width: "800px" });
  fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
  expect(image).toHaveStyle({ width: "1000px" });
  fireEvent.click(screen.getByRole("button", { name: "Rotate" }));
  expect(image).toHaveStyle({ transform: "rotate(90deg)" });
  fireEvent.error(image);
  expect(screen.getByRole("alert")).toHaveTextContent("could not be displayed");
});

const notebook = JSON.stringify({
  metadata: { language_info: { name: "python" } },
  cells: [
    { cell_type: "markdown", source: ["# Title\n", "Words"] },
    {
      cell_type: "code",
      execution_count: 3,
      source: "print(1)",
      outputs: [
        { output_type: "stream", name: "stdout", text: ["1\n"] },
        { output_type: "display_data", data: { "image/png": "QUJD\n" } },
        {
          output_type: "execute_result",
          data: {
            "text/html": "<script>window.hacked = true</script>",
            "text/plain": "frame",
          },
        },
        {
          output_type: "error",
          ename: "ValueError",
          evalue: "bad",
          traceback: ["\u001b[31mValueError\u001b[0m: bad"],
        },
      ],
    },
  ],
});

it("renders cells and outputs but never runs saved HTML", () => {
  const { container } = render(
    <NotebookViewer text={notebook} path="a.ipynb" />,
  );
  expect(screen.getByText(/Title/)).toBeVisible();
  expect(screen.getByText("[3]")).toBeVisible();
  expect(screen.getByRole("img", { name: "Notebook output" })).toHaveAttribute(
    "src",
    "data:image/png;base64,QUJD",
  );
  expect(screen.getByText("frame")).toBeVisible();
  expect(screen.getByText("ValueError: bad")).toBeVisible();
  expect(container.querySelector("script")).toBeNull();
});

it("reads large notebooks whole and rejects files that are not notebooks", async () => {
  const load = vi.fn(async () => notebook);
  render(<NotebookViewer text={null} load={load} path="a.ipynb" />);
  await screen.findByText("[3]");
  expect(parseNotebook("{}")).toBeNull();
  expect(parseNotebook("not json")).toBeNull();
  render(<NotebookViewer text="{}" path="b.ipynb" />);
  expect(screen.getByRole("alert")).toHaveTextContent("could not be read");
});

it("pages tables, switches sheets and marks missing cells", async () => {
  const table = vi.fn(
    async (_path: string, sheet: string | null, start: number) => ({
      columns: ["name", "value"],
      rows: [[`row ${start}`, null]],
      start,
      pageSize: 200,
      totalRows: 450,
      sheet: sheet ?? "First",
      sheets: ["First", "Second"],
    }),
  );
  const adapter: FileAdapter = { id: "t", read: vi.fn(), table };
  render(<TableViewer adapter={adapter} path="a.xlsx" version="1" />);
  expect(await screen.findByText("row 0")).toBeVisible();
  expect(screen.getByText("450 rows · 2 columns")).toBeVisible();
  expect(screen.getByText("·")).toHaveAttribute("data-missing", "true");
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  expect(await screen.findByText("row 200")).toBeVisible();
  fireEvent.change(screen.getByLabelText("Worksheet"), {
    target: { value: "Second" },
  });
  await waitFor(() =>
    expect(table).toHaveBeenLastCalledWith("a.xlsx", "Second", 0),
  );
});

it("explains a table it cannot read", async () => {
  const adapter: FileAdapter = {
    id: "t",
    read: vi.fn(),
    table: vi.fn(async () => {
      throw { message: "Rows have different lengths" };
    }),
  };
  render(<TableViewer adapter={adapter} path="a.xlsx" version="1" />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Rows have different lengths",
  );
});

function delimitedPage(
  rows: (string | null)[][],
  preview: Partial<TablePreview> = {},
  overrides: Partial<FileTable> = {},
): FileTable {
  const start = overrides.start ?? 0;
  return {
    columns: ["id", "value"],
    rows,
    start,
    pageSize: 100,
    totalRows: start + rows.length,
    sheet: null,
    sheets: [],
    preview: {
      nextOffset: null,
      endReached: true,
      columnCount: 2,
      clipped: [],
      approximate: false,
      stamp: "1",
      ...preview,
    },
    ...overrides,
  };
}
const numberedRows = (from: number, to: number) =>
  Array.from({ length: to - from }, (_, index) => [`row ${from + index}`, "x"]);
const tableAdapter = (table: FileAdapter["table"]): FileAdapter => ({
  id: "t",
  read: vi.fn(),
  table,
});

it("adds the next rows below the loaded ones and ends with the exact total", async () => {
  const table = vi.fn(
    async (
      _path: string,
      _sheet: string | null,
      start: number,
      resume?: { offset: number; stamp: string },
    ) =>
      !resume
        ? delimitedPage(numberedRows(0, 100), {
            nextOffset: 700,
            endReached: false,
          })
        : delimitedPage(
            numberedRows(100, 142),
            { stamp: resume.stamp },
            { start, columns: [] },
          ),
  );
  render(
    <TableViewer adapter={tableAdapter(table)} path="a.csv" version="1" />,
  );
  expect(await screen.findByText("First 100 rows · 2 columns")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Next" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Load more" }));
  expect(await screen.findByText("142 rows · 2 columns")).toBeVisible();
  expect(table).toHaveBeenLastCalledWith("a.csv", null, 100, {
    offset: 700,
    stamp: "1",
  });
  expect(screen.getByText("row 0")).toBeVisible();
  expect(screen.getByText("row 141")).toBeVisible();
  expect(screen.getByRole("columnheader", { name: "value" })).toBeVisible();
  expect(screen.getByRole("rowheader", { name: "142" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
});

it("offers no more rows when the file has exactly one page", async () => {
  const table = vi.fn(async () => delimitedPage(numberedRows(0, 100)));
  render(
    <TableViewer adapter={tableAdapter(table)} path="a.csv" version="1" />,
  );
  expect(await screen.findByText("100 rows · 2 columns")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
});

it("drops a page of the old file when the file changes during Load more", async () => {
  let finishOld: (page: FileTable) => void = () => undefined;
  const table = vi.fn(
    (
      _path: string,
      _sheet: string | null,
      _start: number,
      resume?: { offset: number; stamp: string },
    ) => {
      if (resume)
        return new Promise<FileTable>((resolve) => (finishOld = resolve));
      return Promise.resolve(
        delimitedPage(
          table.mock.calls.length > 1 ? [["new", "x"]] : numberedRows(0, 100),
          table.mock.calls.length > 1
            ? { stamp: "2" }
            : { nextOffset: 700, endReached: false },
        ),
      );
    },
  );
  const adapter = tableAdapter(table);
  const { rerender } = render(
    <TableViewer adapter={adapter} path="a.csv" version="1" />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Load more" }));
  rerender(<TableViewer adapter={adapter} path="a.csv" version="2" />);
  expect(await screen.findByText("new")).toBeVisible();
  finishOld(delimitedPage([["stale", "x"]], {}, { start: 100 }));
  await new Promise((resolve) => setTimeout(resolve));
  expect(screen.queryByText("stale")).toBeNull();
  expect(screen.getByText("1 row · 2 columns")).toBeVisible();
});

it("reads from the top when the file changed before the app noticed", async () => {
  const table = vi
    .fn()
    .mockResolvedValueOnce(
      delimitedPage(numberedRows(0, 100), {
        nextOffset: 700,
        endReached: false,
      }),
    )
    .mockRejectedValueOnce({ code: "file_changed", message: "changed" })
    .mockResolvedValueOnce(delimitedPage([["new", "x"]], { stamp: "2" }));
  render(
    <TableViewer adapter={tableAdapter(table)} path="a.csv" version="1" />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Load more" }));
  expect(await screen.findByText("new")).toBeVisible();
  expect(screen.queryByText("row 0")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(table).toHaveBeenLastCalledWith("a.csv", null, 0);
});

it("keeps the loaded rows and says why when Load more fails", async () => {
  const table = vi
    .fn()
    .mockResolvedValueOnce(
      delimitedPage(numberedRows(0, 100), {
        nextOffset: 700,
        endReached: false,
      }),
    )
    .mockRejectedValueOnce({ message: "The disk went away" });
  render(
    <TableViewer adapter={tableAdapter(table)} path="a.csv" version="1" />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Load more" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The disk went away",
  );
  expect(screen.getByText("row 0")).toBeVisible();
  expect(screen.getByRole("button", { name: "Load more" })).toBeVisible();
});

it("shows short and long rows without moving any cell", async () => {
  const table = vi.fn(async () =>
    delimitedPage([["only"], ["1", "2", "3"], ["", "kept"]], {
      columnCount: 3,
    }),
  );
  render(
    <TableViewer adapter={tableAdapter(table)} path="a.csv" version="1" />,
  );
  expect(await screen.findByText("3 rows · 3 columns")).toBeVisible();
  const rows = screen.getAllByRole("row");
  expect(rows[0].querySelectorAll("th")).toHaveLength(4);
  expect(rows[1].querySelectorAll("td")[1]).toHaveAttribute(
    "data-missing",
    "true",
  );
  expect(rows[1].querySelectorAll("td")).toHaveLength(3);
  expect(rows[2].querySelectorAll("td")[2]).toHaveTextContent("3");
  expect(rows[3].querySelectorAll("td")[0]).toHaveAttribute(
    "data-missing",
    "false",
  );
});

it("marks clipped cells and says when columns are hidden or text is approximate", async () => {
  const table = vi.fn(async () =>
    delimitedPage([["short", "long text"]], {
      clipped: [
        [null, 0],
        [0, 1],
      ],
      columnCount: 900,
      approximate: true,
    }),
  );
  render(
    <TableViewer adapter={tableAdapter(table)} path="a.csv" version="1" />,
  );
  expect(
    await screen.findByText("1 row · 2 of 900 columns shown"),
  ).toBeVisible();
  expect(screen.getByText(/not UTF-8/)).toBeVisible();
  expect(screen.getByRole("columnheader", { name: /id/ })).toHaveTextContent(
    "Clippedid",
  );
  expect(screen.getByText("short")).not.toHaveTextContent("Clipped");
  expect(screen.getByRole("cell", { name: /long text/ })).toHaveTextContent(
    "Clippedlong text",
  );
});

it("does not imply a total when a row is too long to pass", async () => {
  const table = vi.fn(async () =>
    delimitedPage(numberedRows(0, 7), { endReached: false }),
  );
  render(
    <TableViewer adapter={tableAdapter(table)} path="a.csv" version="1" />,
  );
  expect(await screen.findByText("First 7 rows · 2 columns")).toBeVisible();
  expect(screen.getByText(/next row is too long/)).toBeVisible();
  expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
});

it("says when a delimited file is empty or has only a header", async () => {
  const table = vi
    .fn()
    .mockResolvedValueOnce(
      delimitedPage([], { columnCount: 0 }, { columns: [] }),
    )
    .mockResolvedValueOnce(delimitedPage([]));
  const adapter = tableAdapter(table);
  const { rerender } = render(
    <TableViewer adapter={adapter} path="a.csv" version="1" />,
  );
  expect(await screen.findByText("This file is empty")).toBeVisible();
  rerender(<TableViewer adapter={adapter} path="a.csv" version="2" />);
  expect(await screen.findByText("No rows · 2 columns")).toBeVisible();
});

it("renders HTML in an empty sandbox and says when a draft is not shown", () => {
  render(<HtmlViewer src="pipeline-file://t" title="a.html" stale />);
  const frame = screen.getByTitle("Rendered a.html");
  expect(frame).toHaveAttribute("sandbox", "");
  expect(screen.getByRole("note")).toHaveTextContent("not your draft");
});

it("offers another route when media cannot play", () => {
  render(<MediaViewer src="x" title="clip.mov" video />);
  fireEvent.error(screen.getByLabelText("clip.mov"));
  expect(screen.getByRole("alert")).toHaveTextContent("Open it externally");
});
