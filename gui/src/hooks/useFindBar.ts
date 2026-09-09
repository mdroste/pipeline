import {
  useState,
  useCallback,
  useEffect,
  useRef,
  type RefObject,
} from "react";

/** Highlighting is DOM work proportional to the match count; cap it so a
 *  short query over a huge report cannot freeze the UI. */
export const MAX_FIND_MARKS = 1000;

/** Remove every `<mark class="search-hit">` inside `container`, restoring the
 *  original text so a fresh search starts clean. Exported for testing. */
export function clearHighlights(container: HTMLElement): void {
  const marks = container.querySelectorAll("mark.search-hit");
  marks.forEach((m) => {
    const parent = m.parentNode;
    if (!parent) return;
    parent.replaceChild(document.createTextNode(m.textContent || ""), m);
    parent.normalize();
  });
}

/** Wrap case-insensitive occurrences of `query` in `container` in a
 *  `<mark class="search-hit">`, skipping script/style, KaTeX's visually hidden
 *  MathML source, and already-marked nodes. At most `limit` occurrences are
 *  marked; `capped` reports whether any were left unmarked. Marks are in
 *  document order. Exported for testing. */
export function applyHighlights(
  container: HTMLElement,
  query: string,
  limit: number = MAX_FIND_MARKS,
): { marks: HTMLElement[]; capped: boolean } {
  const q = query.toLowerCase();
  if (!q) return { marks: [], capped: false };
  const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => {
      const text = node.textContent;
      if (!text || !text.toLowerCase().includes(q))
        return NodeFilter.FILTER_REJECT;
      const parent = (node as Text).parentElement;
      if (
        parent &&
        (parent.tagName === "SCRIPT" || parent.tagName === "STYLE")
      ) {
        return NodeFilter.FILTER_REJECT;
      }
      // KaTeX renders a hidden MathML copy of the raw LaTeX; matches inside it
      // are invisible, so counting or jumping to them strands the user.
      if (parent?.closest(".katex-mathml")) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  const textNodes: Text[] = [];
  let n: Node | null;
  while ((n = walker.nextNode())) textNodes.push(n as Text);

  const marks: HTMLElement[] = [];
  let capped = false;
  for (const textNode of textNodes) {
    if (marks.length >= limit) {
      capped = true;
      break;
    }
    const text = textNode.textContent || "";
    const lower = text.toLowerCase();
    const frag = document.createDocumentFragment();
    let from = 0;
    let idx = lower.indexOf(q, from);
    if (idx === -1) continue;
    while (idx !== -1) {
      if (marks.length >= limit) {
        capped = true;
        break;
      }
      if (idx > from)
        frag.appendChild(document.createTextNode(text.slice(from, idx)));
      const mark = document.createElement("mark");
      mark.className = "search-hit";
      mark.textContent = text.slice(idx, idx + q.length);
      frag.appendChild(mark);
      marks.push(mark);
      from = idx + q.length;
      idx = lower.indexOf(q, from);
    }
    if (from < text.length)
      frag.appendChild(document.createTextNode(text.slice(from)));
    textNode.parentNode?.replaceChild(frag, textNode);
  }
  return { marks, capped };
}

/** Find-bar state and behavior over a scrollable content container.
 *  Ctrl/Cmd-F opens it; Escape closes; Enter/Shift-Enter cycle matches. */
export function useFindBar(
  containerRef: RefObject<HTMLElement | null>,
  resetKey: unknown,
) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [count, setCount] = useState(0);
  const [capped, setCapped] = useState(false);
  const [current, setCurrent] = useState(0);
  const marksRef = useRef<HTMLElement[]>([]);

  const focusMark = useCallback((i: number) => {
    const marks = marksRef.current;
    marks.forEach((m) => m.classList.remove("current"));
    const mark = marks[i];
    if (mark) {
      mark.classList.add("current");
      mark.scrollIntoView?.({ block: "center", behavior: "smooth" });
    }
  }, []);

  // Re-run the highlight whenever the query or the content changes.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    clearHighlights(container);
    if (!open || !query) {
      marksRef.current = [];
      setCount(0);
      setCapped(false);
      setCurrent(0);
      return;
    }
    const { marks, capped: hitCap } = applyHighlights(container, query);
    marksRef.current = marks;
    setCount(marks.length);
    setCapped(hitCap);
    setCurrent(marks.length > 0 ? 0 : 0);
    if (marks.length > 0) focusMark(0);
    return () => clearHighlights(container);
  }, [query, open, resetKey, containerRef, focusMark]);

  const next = useCallback(() => {
    setCurrent((c) => {
      if (marksRef.current.length === 0) return 0;
      const nc = (c + 1) % marksRef.current.length;
      focusMark(nc);
      return nc;
    });
  }, [focusMark]);

  const prev = useCallback(() => {
    setCurrent((c) => {
      if (marksRef.current.length === 0) return 0;
      const nc = (c - 1 + marksRef.current.length) % marksRef.current.length;
      focusMark(nc);
      return nc;
    });
  }, [focusMark]);

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
  }, []);

  // Ctrl/Cmd-F opens the find bar (scoped to when the container is mounted).
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
        if (
          e.defaultPrevented ||
          !containerRef.current ||
          containerRef.current.closest(".hidden, [hidden]")
        )
          return;
        const pane =
          e.target instanceof Element
            ? e.target.closest(".file-pane, .pdf-reader")
            : null;
        if (pane && !pane.contains(containerRef.current)) return;
        e.preventDefault();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [containerRef]);

  return {
    open,
    setOpen,
    query,
    setQuery,
    count,
    capped,
    current,
    next,
    prev,
    close,
  };
}
