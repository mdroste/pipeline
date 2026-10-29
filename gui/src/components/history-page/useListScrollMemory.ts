import { useEffect, useRef } from "react";

/** The nearest ancestor that scrolls (the app's content pane). */
function scrollParent(node: HTMLElement | null): HTMLElement | null {
  for (
    let parent = node?.parentElement;
    parent;
    parent = parent.parentElement
  ) {
    const { overflowY } = getComputedStyle(parent);
    if (overflowY === "auto" || overflowY === "scroll") return parent;
  }
  return null;
}

/**
 * Keeps the history list's reading position. A new page or filter starts at
 * the top; returning from a report or comparison restores where the list was.
 *
 * `resetKey` changes with the page, filters and sort order. `shown` is true
 * while the list's rows are on screen. Call `remember` before leaving the list.
 */
export default function useListScrollMemory(resetKey: string, shown: boolean) {
  const rootRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const savedRef = useRef<{ list: number; page: number } | null>(null);

  useEffect(() => {
    if (listRef.current) listRef.current.scrollTop = 0;
  }, [resetKey]);

  // A passive effect, so it runs after the app shell resets its own scroll
  // position on navigation and the restored offset wins.
  useEffect(() => {
    const saved = savedRef.current;
    if (!shown || !saved) return;
    savedRef.current = null;
    if (listRef.current) listRef.current.scrollTop = saved.list;
    const page = scrollParent(rootRef.current);
    if (page) page.scrollTop = saved.page;
  }, [shown]);

  const remember = () => {
    savedRef.current = {
      list: listRef.current?.scrollTop ?? 0,
      page: scrollParent(rootRef.current)?.scrollTop ?? 0,
    };
  };

  return { rootRef, listRef, remember };
}
