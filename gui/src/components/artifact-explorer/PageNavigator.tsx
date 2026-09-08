import { memo, useEffect, useMemo, useState } from "react";
import type { ArtifactEntry, PageArtifactIndex } from "../../lib/artifactTypes";
import {
  MAX_INDEXED_PAGES,
  pageNumber,
  pageSelection,
  selectedPage,
} from "./navigation";
export const PageBrowser = memo(function PageBrowser({
  items,
  index,
  onSelect,
  selected,
}: {
  items: ArtifactEntry[];
  index?: PageArtifactIndex | null;
  onSelect: (path: string) => void;
  selected: string;
}) {
  const sorted = useMemo(
    () => [...items].sort((a, b) => pageNumber(a) - pageNumber(b)),
    [items],
  );
  const indexedCount =
    index && Number.isSafeInteger(index.count)
      ? Math.min(Math.max(index.count, 0), MAX_INDEXED_PAGES)
      : 0;
  const legacyPages = useMemo(
    () =>
      sorted
        .map((item) => ({ number: pageNumber(item), item }))
        .filter(
          ({ number }) =>
            Number.isFinite(number) && number !== Number.MAX_SAFE_INTEGER,
        ),
    [sorted],
  );
  const selectedNumber = useMemo(() => {
    if (index) {
      const page = selectedPage(selected);
      return page !== null && page <= indexedCount ? page : null;
    }
    return (
      legacyPages.find(({ item }) => item.rel_path === selected)?.number ?? null
    );
  }, [index, indexedCount, legacyPages, selected]);
  const [draftPage, setDraftPage] = useState("");

  useEffect(() => {
    setDraftPage(selectedNumber?.toString() ?? "");
  }, [selectedNumber]);

  const lastPage = index
    ? indexedCount
    : (legacyPages[legacyPages.length - 1]?.number ?? 0);
  const selectedLegacyIndex = index
    ? -1
    : legacyPages.findIndex(({ number }) => number === selectedNumber);
  const hasPrevious =
    selectedNumber !== null &&
    (index ? selectedNumber > 1 : selectedLegacyIndex > 0);
  const hasNext =
    lastPage > 0 &&
    (selectedNumber === null ||
      (index
        ? selectedNumber < indexedCount
        : selectedLegacyIndex < legacyPages.length - 1));

  const selectPage = (page: number) => {
    if (index) {
      if (Number.isInteger(page) && page >= 1 && page <= indexedCount) {
        onSelect(pageSelection(page));
        return true;
      }
      return false;
    }
    const match = legacyPages.find(({ number }) => number === page);
    if (!match) return false;
    onSelect(match.item.rel_path);
    return true;
  };

  const commitDraft = () => {
    const page = Number(draftPage);
    if (!Number.isInteger(page) || !selectPage(page)) {
      setDraftPage(selectedNumber?.toString() ?? "");
    }
  };

  const move = (direction: -1 | 1) => {
    if (selectedNumber === null) {
      if (direction === 1) {
        selectPage(index ? 1 : (legacyPages[0]?.number ?? 0));
      }
      return;
    }
    if (index) {
      selectPage(selectedNumber + direction);
      return;
    }
    const target = legacyPages[selectedLegacyIndex + direction];
    if (target) selectPage(target.number);
  };

  return (
    <div
      className="flex items-center overflow-hidden rounded-md border border-gray-200 bg-white
                 dark:border-gray-700 dark:bg-gray-900"
    >
      <button
        type="button"
        onClick={() => move(-1)}
        disabled={!hasPrevious}
        aria-label="Previous page"
        title="Previous page"
        className="shrink-0 border-r border-gray-200 px-1.5 py-1.5 text-gray-500 transition-colors
                   enabled:hover:bg-gray-100 enabled:hover:text-gray-800 disabled:opacity-30
                   dark:border-gray-700 dark:text-gray-400 dark:enabled:hover:bg-gray-800 dark:enabled:hover:text-gray-100"
      >
        <svg
          aria-hidden="true"
          className="h-3 w-3"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m15 18-6-6 6-6"
          />
        </svg>
      </button>
      <form
        className="flex min-w-0 flex-1 items-center"
        onSubmit={(event) => {
          event.preventDefault();
          commitDraft();
        }}
      >
        <input
          type="text"
          inputMode="numeric"
          pattern="[0-9]*"
          value={draftPage}
          onChange={(event) => setDraftPage(event.target.value)}
          onBlur={commitDraft}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              setDraftPage(selectedNumber?.toString() ?? "");
              event.currentTarget.blur();
            }
          }}
          aria-label="Page number"
          placeholder="Page"
          title={
            lastPage > 0
              ? `Enter a page from 1 to ${lastPage}`
              : "No pages available"
          }
          className="min-w-0 flex-1 bg-transparent px-2 py-1 text-right text-xs tabular-nums text-gray-700
                     outline-none placeholder:text-gray-400 dark:text-gray-200 dark:placeholder:text-gray-600"
        />
        <span className="shrink-0 pr-2 text-[11px] tabular-nums text-gray-400 dark:text-gray-500">
          / {lastPage}
        </span>
      </form>
      <button
        type="button"
        onClick={() => move(1)}
        disabled={!hasNext}
        aria-label="Next page"
        title="Next page"
        className="shrink-0 border-l border-gray-200 px-1.5 py-1.5 text-gray-500 transition-colors
                   enabled:hover:bg-gray-100 enabled:hover:text-gray-800 disabled:opacity-30
                   dark:border-gray-700 dark:text-gray-400 dark:enabled:hover:bg-gray-800 dark:enabled:hover:text-gray-100"
      >
        <svg
          aria-hidden="true"
          className="h-3 w-3"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m9 18 6-6-6-6"
          />
        </svg>
      </button>
    </div>
  );
});
