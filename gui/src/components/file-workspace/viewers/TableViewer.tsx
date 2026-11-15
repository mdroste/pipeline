// Paged spreadsheet rows, or the loaded start of a CSV or TSV file. There is no
// sorting or filtering: acting on loaded rows would misrepresent the file.
import { useEffect, useRef, useState } from "react";
import type { FileAdapter, FileTable } from "../../../lib/fileWorkspaceClient";
import { workbenchErrorMessage } from "../../../lib/workbenchError";
import ReportIssueLink from "../../ReportIssueLink";

/** Load more stops here, so the page stays usable. */
const MAX_LOADED_ROWS = 10_000;

const count = (value: number, noun: string) =>
  `${value.toLocaleString()} ${noun}${value === 1 ? "" : "s"}`;

/** Add the next page. A stale page must never reach this. */
function appended(table: FileTable, page: FileTable): FileTable {
  return {
    ...page,
    columns: table.columns,
    start: table.start,
    rows: [...table.rows, ...page.rows],
    preview: page.preview && {
      ...page.preview,
      columnCount: Math.max(
        table.preview?.columnCount ?? 0,
        page.preview.columnCount,
      ),
      clipped: [...(table.preview?.clipped ?? []), ...page.preview.clipped],
      approximate:
        Boolean(table.preview?.approximate) || page.preview.approximate,
    },
  };
}
/** What the toolbar may say. A total appears only when the end was reached. */
function summary(table: FileTable, shownColumns: number) {
  const { preview, rows } = table;
  if (!preview) return "";
  if (preview.endReached && !preview.columnCount) return "This file is empty";
  const read = preview.endReached
    ? table.totalRows
      ? count(table.totalRows, "row")
      : "No rows"
    : rows.length
      ? `First ${count(rows.length, "row")}`
      : "No rows shown";
  const columns =
    preview.columnCount > shownColumns
      ? `${shownColumns.toLocaleString()} of ${count(preview.columnCount, "column")} shown`
      : count(preview.columnCount, "column");
  return `${read} · ${columns}`;
}

interface Props {
  adapter: FileAdapter;
  path: string;
  /** Changes when the file does. */
  version: string;
}

export default function TableViewer(props: Props) {
  // A different file starts with its own cursor; a new version of this
  // file keeps the current sheet and page whenever they still exist.
  return (
    <TablePages
      key={JSON.stringify([props.adapter.id, props.path])}
      {...props}
    />
  );
}

function TablePages({ adapter, path, version }: Props) {
  const [table, setTable] = useState<FileTable | null>(null);
  const cursor = useRef<{ sheet: string | null; start: number }>({
    sheet: null,
    start: 0,
  });
  const openedVersion = useRef<string | null>(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [moreError, setMoreError] = useState("");
  const [restarts, setRestarts] = useState(0);
  /** Each read from the top starts a new run. Only its run's pages are kept. */
  const run = useRef(0);
  const scroller = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const current = ++run.current;
    setError("");
    setMoreError("");
    setLoading(true);
    const { sheet, start } = cursor.current;
    const changed =
      openedVersion.current !== null && openedVersion.current !== version;
    const read = async () => {
      if (!adapter.table) throw new Error("This table cannot be read here.");
      let value: FileTable;
      let message = "";
      try {
        value = await adapter.table(path, sheet, start);
      } catch (cause) {
        if (!changed || !sheet || current !== run.current) throw cause;
        // Only recover if the new sheet list confirms the sheet is gone.
        // A damaged but still-present worksheet must show its read error.
        const first = await adapter.table(path, null, 0);
        if (first.sheets.includes(sheet)) throw cause;
        value = first;
        message = `Worksheet ${sheet} is no longer in this file. Showing ${first.sheet ?? "the first worksheet"}.`;
      }
      if (current !== run.current) return;
      if (!value.preview && value.start > 0 && value.start >= value.totalRows) {
        value = await adapter.table(path, sheet, 0);
        message = "The worksheet has fewer rows. Showing its first page.";
      }
      if (current !== run.current) return;
      openedVersion.current = version;
      cursor.current = {
        sheet: sheet ? value.sheet : null,
        start: value.start,
      };
      setTable(value);
      setNotice(message);
      setLoading(false);
    };
    void read().catch((e) => {
      if (current !== run.current) return;
      setTable(null);
      setLoading(false);
      setError(workbenchErrorMessage(e));
    });
    return () => {
      run.current++;
    };
  }, [adapter, path, version, restarts]);
  const goTo = (sheet: string | null, start: number) => {
    cursor.current = { sheet, start };
    setRestarts((value) => value + 1);
  };
  const loadMore = () => {
    const offset = table?.preview?.nextOffset ?? null;
    if (loading || !table?.preview || offset === null) return;
    const current = run.current;
    setLoading(true);
    setMoreError("");
    adapter
      .table?.(path, null, table.start + table.rows.length, {
        offset,
        stamp: table.preview.stamp,
      })
      .then(
        (page) => {
          if (current !== run.current) return;
          setTable((old) => old && appended(old, page));
          setLoading(false);
          // The button is about to go, so keep the keyboard in the table.
          if ((page.preview?.nextOffset ?? null) === null)
            scroller.current?.focus({ preventScroll: true });
        },
        (e) => {
          if (current !== run.current) return;
          setLoading(false);
          if ((e as { code?: unknown } | null)?.code === "file_changed")
            setRestarts((value) => value + 1);
          else setMoreError(workbenchErrorMessage(e));
        },
      );
  };
  if (error)
    return (
      <p role="alert" className="file-error">
        {error}
        <button onClick={() => goTo(null, 0)}>Reload table</button>
        <ReportIssueLink />
      </p>
    );
  if (!table) return <p className="file-status">Reading table…</p>;
  const last = Math.min(table.totalRows, table.start + table.rows.length);
  const { preview } = table;
  // A delimited file's rows can differ in length. Each row fills the widest one,
  // so a cell that is not in the file reads as missing, not as an empty cell.
  const width = preview
    ? table.rows.reduce(
        (widest, row) => Math.max(widest, row.length),
        table.columns.length,
      )
    : table.columns.length;
  const shown = (row: (string | null)[]) =>
    preview
      ? Array.from({ length: width }, (_, column) => row[column] ?? null)
      : row;
  const clipped = new Set(
    preview?.clipped.map(([row, column]) => `${row}:${column}`),
  );
  const marker = (
    <span
      className="file-table-clipped"
      title="This cell is longer than Pipeline shows here. The file is unchanged."
    >
      Clipped
    </span>
  );
  const atLimit = table.rows.length >= MAX_LOADED_ROWS;
  const canLoadMore = (preview?.nextOffset ?? null) !== null && !atLimit;
  const blocked = preview && !preview.endReached && preview.nextOffset === null;
  return (
    <div className="file-table">
      <div className="file-toolbar" role="toolbar" aria-label="Table view">
        {table.sheets.length > 1 && (
          <select
            aria-label="Worksheet"
            value={table.sheet ?? ""}
            onChange={(event) => goTo(event.target.value, 0)}
          >
            {table.sheets.map((name) => (
              <option key={name}>{name}</option>
            ))}
          </select>
        )}
        {preview ? (
          <>
            <span aria-live="polite">{summary(table, width)}</span>
            {canLoadMore && (
              <button aria-disabled={loading} onClick={loadMore}>
                Load more
              </button>
            )}
            {loading && <span className="file-table-note">Reading…</span>}
            {blocked && (
              <span className="file-table-note">
                The next row is too long to show here.
              </span>
            )}
            {atLimit && !blocked && !preview.endReached && (
              <span className="file-table-note">
                Open the file externally to see more rows.
              </span>
            )}
            {preview.approximate && (
              <span className="file-table-note">
                This file is not UTF-8, so some characters may be wrong.
              </span>
            )}
          </>
        ) : (
          <>
            <span>
              {table.totalRows.toLocaleString()} rows ·{" "}
              {table.columns.length.toLocaleString()} columns
            </span>
            <button
              disabled={table.start === 0}
              onClick={() =>
                goTo(
                  cursor.current.sheet,
                  Math.max(0, table.start - table.pageSize),
                )
              }
            >
              Previous
            </button>
            <span aria-live="polite">
              {table.rows.length
                ? `${(table.start + 1).toLocaleString()}–${last.toLocaleString()}`
                : "No rows"}
            </span>
            <button
              disabled={last >= table.totalRows}
              onClick={() =>
                goTo(cursor.current.sheet, table.start + table.pageSize)
              }
            >
              Next
            </button>
          </>
        )}
      </div>
      {notice && (
        <p role="status" className="file-table-note">
          {notice}
        </p>
      )}
      {moreError && (
        <p role="alert" className="file-error">
          {moreError}
          <ReportIssueLink />
        </p>
      )}
      <div className="file-table-scroll" tabIndex={0} ref={scroller}>
        <table aria-label={`Table: ${path}`}>
          <thead>
            <tr>
              <th scope="col">#</th>
              {table.columns.map((column, index) => (
                <th key={index} scope="col" title={preview && column}>
                  {clipped.has(`null:${index}`) && marker}
                  {column}
                </th>
              ))}
              {Array.from(
                { length: width - table.columns.length },
                (_, index) => (
                  <th key={index} scope="col" aria-label="No header" />
                ),
              )}
            </tr>
          </thead>
          <tbody>
            {table.rows.map((row, index) => (
              <tr key={table.start + index}>
                <th scope="row">{table.start + index + 1}</th>
                {shown(row).map((value, column) => (
                  <td
                    key={column}
                    data-missing={value === null}
                    title={preview ? (value ?? undefined) : undefined}
                  >
                    {clipped.has(`${table.start + index}:${column}`) && marker}
                    {value ?? "·"}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
