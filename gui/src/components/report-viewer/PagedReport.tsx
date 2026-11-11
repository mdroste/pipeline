import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { splitReport } from "../../lib/reportChunks";
import type { ReportNavigationEntry } from "../../lib/reportNavigation";
import { buildNavigationIndex } from "../../lib/reportNavigation";
import { applyHighlights, clearHighlights } from "../../hooks/useFindBar";

function PlainPart({ markdown, query }: { markdown: string; query?: string }) {
  const ref = useRef<HTMLPreElement>(null);
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    clearHighlights(element);
    if (query)
      applyHighlights(element, query).marks[0]?.scrollIntoView?.({
        block: "center",
      });
    return () => clearHighlights(element);
  }, [query, markdown]);
  return (
    <div className="h-full overflow-auto p-5">
      <p className="mb-3 text-xs text-ink-muted">
        This large block is shown as source text.
      </p>
      <pre ref={ref} className="whitespace-pre-wrap break-words text-sm">
        {markdown}
      </pre>
    </div>
  );
}

/** Only the selected part is rendered; all parts stay available to find and contents. */
export default function PagedReport({
  markdown,
  scrollRequest,
  render,
}: {
  markdown: string;
  scrollRequest?: { key: number; heading: string } | null;
  render: (
    markdown: string,
    request?: { key: number; heading: string },
    findRequest?: { key: number; query: string },
  ) => ReactNode;
}) {
  const chunks = useMemo(() => splitReport(markdown), [markdown]);
  const [page, setPage] = useState(0);
  const [query, setQuery] = useState("");
  const [target, setTarget] = useState<{ key: number; heading: string }>();
  const [findRequest, setFindRequest] = useState<{
    key: number;
    query: string;
  }>();
  const selected = Math.min(page, chunks.length - 1);
  const current = chunks[selected];
  const search = useRef<HTMLInputElement>(null);
  const contents = useMemo(
    () =>
      chunks.flatMap((chunk, page) =>
        chunk.plain
          ? []
          : buildNavigationIndex(chunk.markdown).entries.map((entry) => ({
              entry,
              page,
            })),
      ),
    [chunks],
  );
  const matches = useMemo(
    () =>
      query.trim()
        ? chunks
            .map((chunk, page) => ({ chunk, page }))
            .filter(({ chunk }) =>
              chunk.markdown
                .toLocaleLowerCase()
                .includes(query.trim().toLocaleLowerCase()),
            )
        : [],
    [chunks, query],
  );
  const open = (page: number, entry?: ReportNavigationEntry) => {
    setPage(page);
    setFindRequest(undefined);
    setTarget(entry ? { key: Date.now(), heading: entry.text } : undefined);
  };
  useEffect(() => {
    setPage(0);
    setQuery("");
    setTarget(undefined);
  }, [markdown]);
  useEffect(() => {
    if (!scrollRequest) return;
    const match = contents.find(
      ({ entry }) => entry.text === scrollRequest.heading,
    );
    if (match) {
      setPage(match.page);
      setTarget(scrollRequest);
    }
  }, [scrollRequest, contents]);
  return (
    <div
      className="flex h-full min-h-0 flex-col"
      onKeyDownCapture={(event) => {
        if (
          (event.ctrlKey || event.metaKey) &&
          event.key.toLowerCase() === "f"
        ) {
          event.preventDefault();
          event.stopPropagation();
          search.current?.focus();
        }
      }}
      onClickCapture={(event) => {
        const anchor =
          event.target instanceof Element
            ? event.target.closest("a[href^='#']")
            : null;
        if (!anchor) return;
        let id: string;
        try {
          id = decodeURIComponent(anchor.getAttribute("href")!.slice(1));
        } catch {
          return;
        }
        const match = contents.find(
          ({ entry }) => id === entry.id || id.endsWith("-" + entry.id),
        );
        if (match && match.page !== selected) {
          event.preventDefault();
          event.stopPropagation();
          open(match.page, match.entry);
        }
      }}
    >
      <nav
        aria-label="Report parts"
        className="shrink-0 space-y-2 border-b border-line bg-surface px-4 py-3 text-xs"
      >
        <div className="flex flex-wrap items-center gap-2">
          <label className="min-w-0">
            Part{" "}
            <select
              aria-label="Report part"
              value={selected}
              onChange={(event) => open(Number(event.target.value))}
              className="max-w-64 rounded border border-line bg-surface p-1.5"
            >
              {chunks.map((chunk, index) => (
                <option value={index} key={index}>
                  {index + 1}. {chunk.title}
                </option>
              ))}
            </select>
          </label>
          <span>
            {selected + 1} of {chunks.length}
          </span>
          <button
            className="ui-page-button"
            disabled={selected === 0}
            onClick={() => open(selected - 1)}
          >
            Previous part
          </button>
          <button
            className="ui-page-button"
            disabled={selected === chunks.length - 1}
            onClick={() => open(selected + 1)}
          >
            Next part
          </button>
          <input
            ref={search}
            type="search"
            aria-label="Find across report"
            placeholder="Find across report…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="rounded border border-line bg-surface p-2"
          />
        </div>
        {query.trim() && (
          <div
            className="flex max-h-28 flex-wrap gap-2 overflow-auto"
            role="status"
          >
            {matches.length
              ? matches.map(({ chunk, page }) => (
                  <button
                    className="underline"
                    key={page}
                    onClick={() => {
                      open(page);
                      setFindRequest({ key: Date.now(), query: query.trim() });
                    }}
                  >
                    Part {page + 1}: {chunk.title}
                  </button>
                ))
              : "No matching parts."}
          </div>
        )}
        <details>
          <summary className="cursor-pointer">
            Contents for the full report
          </summary>
          <ol className="mt-2 max-h-48 space-y-1 overflow-auto">
            {contents.map(({ entry, page }, index) => (
              <li key={index}>
                <button
                  className="text-left underline underline-offset-2"
                  onClick={() => open(page, entry)}
                >
                  Part {page + 1} · {entry.text}
                </button>
              </li>
            ))}
          </ol>
        </details>
      </nav>
      <div className="min-h-0 flex-1" key={selected}>
        {current.plain ? (
          <PlainPart markdown={current.markdown} query={findRequest?.query} />
        ) : (
          render(current.markdown, target, findRequest)
        )}
      </div>
    </div>
  );
}
