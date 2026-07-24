import React, { useMemo, useRef } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import { useFindBar } from "../hooks/useFindBar";
import { normalizeMathDelimiters } from "../lib/mathMarkdown";

interface Props {
  markdown: string;
}

/**
 * Catches render errors from rehype-katex/remark-math so malformed `$…$`
 * in LLM output can't crash the whole report view. On error, the children
 * render function is invoked again with `fallback=true` so the caller can
 * re-render without the math plugins. State resets when `resetKey` changes.
 */
class MathErrorBoundary extends React.Component<
  { resetKey: string; children: (fallback: boolean) => React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false };
  static getDerivedStateFromError() {
    return { hasError: true };
  }
  componentDidCatch(error: unknown) {
    console.error("Math rendering failed, falling back to plain markdown:", error);
  }
  componentDidUpdate(prev: { resetKey: string }) {
    if (prev.resetKey !== this.props.resetKey && this.state.hasError) {
      this.setState({ hasError: false });
    }
  }
  render() {
    return this.props.children(this.state.hasError);
  }
}

/** Recursively extract plain text from React children. */
function extractText(node: React.ReactNode): string {
  if (typeof node === "string") return node;
  if (typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (node && typeof node === "object" && "props" in node) {
    return extractText((node as React.ReactElement).props.children);
  }
  return "";
}

/**
 * If `children` starts with a `#N.` prefix in its leading text node, strip it
 * and return `{ num, rest }` — where `rest` preserves the original React nodes
 * (so rendered math/formatting survives). Returns null otherwise.
 */
function splitCommentPrefix(
  children: React.ReactNode,
): { num: string; rest: React.ReactNode[] } | null {
  const arr = React.Children.toArray(children);
  if (arr.length === 0) return null;
  const first = arr[0];
  if (typeof first !== "string") return null;
  const match = first.match(/^#(\d+)\.\s*/);
  if (!match) return null;
  const remainder = first.slice(match[0].length);
  const rest = remainder ? [remainder, ...arr.slice(1)] : arr.slice(1);
  return { num: match[1], rest };
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "");
}

export default function ReportViewer({ markdown }: Props) {
  const contentRef = useRef<HTMLDivElement>(null);
  const normalizedMarkdown = useMemo(
    () => normalizeMathDelimiters(markdown),
    [markdown],
  );
  const find = useFindBar(contentRef, normalizedMarkdown);

  // Extract headings for table of contents
  const headings = useMemo(() => {
    const matches = normalizedMarkdown.matchAll(/^(#{1,3})\s+(.+)$/gm);
    return Array.from(matches).map((m) => ({
      level: m[1].length,
      text: m[2],
      id: slugify(m[2]),
    }));
  }, [normalizedMarkdown]);

  return (
    <div className="flex h-full relative">
      {find.open && (
        <div className="absolute top-2 right-3 z-20 flex items-center gap-1 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg shadow-lg px-2 py-1">
          <input
            autoFocus
            value={find.query}
            onChange={(e) => find.setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); e.shiftKey ? find.prev() : find.next(); }
              if (e.key === "Escape") find.close();
            }}
            placeholder="Find in report…"
            className="w-44 py-0.5 px-1.5 text-sm bg-transparent text-gray-900 dark:text-gray-100 focus:outline-none"
          />
          <span className="text-xs text-gray-400 tabular-nums min-w-[3rem] text-right">
            {find.count > 0 ? `${find.current + 1}/${find.count}` : "0/0"}
          </span>
          <button onClick={find.prev} disabled={find.count === 0} className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100 disabled:opacity-30" title="Previous (Shift+Enter)">↑</button>
          <button onClick={find.next} disabled={find.count === 0} className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100 disabled:opacity-30" title="Next (Enter)">↓</button>
          <button onClick={find.close} className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100" title="Close (Esc)">✕</button>
        </div>
      )}
      {/* Table of contents */}
      {headings.length > 3 && (
        <nav className="toc-nav">
          <h4 className="text-[11px] font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-widest mb-4">
            Contents
          </h4>
          <ul className="space-y-0.5">
            {headings
              .filter((h) => h.level <= 2)
              .map((h, i) => (
                <li key={i}>
                  <a
                    href={`#${h.id}`}
                    className={`toc-link ${
                      h.level === 1 ? "toc-h1" : "toc-h2"
                    }`}
                  >
                    {h.text}
                  </a>
                </li>
              ))}
          </ul>
        </nav>
      )}

      {/* Report content */}
      <div className="flex-1 overflow-y-auto">
        <div className="report-content" ref={contentRef}>
          <MathErrorBoundary resetKey={normalizedMarkdown}>
            {(fallback) => (
              <>
                {fallback && (
                  <div className="mb-4 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
                    Some math in this report could not be rendered — formulas
                    are shown as raw LaTeX notation.
                  </div>
                )}
                <ReactMarkdown
                remarkPlugins={fallback ? [remarkGfm] : [remarkGfm, remarkMath]}
                rehypePlugins={fallback ? [] : [rehypeKatex]}
                components={{
                  h1: ({ children, ...props }) => (
                    <h1 id={slugify(extractText(children))} {...props}>
                      {children}
                    </h1>
                  ),
                  h2: ({ children, ...props }) => (
                    <h2 id={slugify(extractText(children))} {...props}>
                      {children}
                    </h2>
                  ),
                  h3: ({ children, ...props }) => (
                    <h3 id={slugify(extractText(children))} {...props}>
                      {children}
                    </h3>
                  ),
                  // Detect comment headers: a paragraph whose only child is
                  // <strong>#N. Title</strong>  →  render as a styled card.
                  p: ({ children, node, ...props }) => {
                    const childArray = React.Children.toArray(children);
                    if (childArray.length === 1 && React.isValidElement(childArray[0])) {
                      const child = childArray[0] as React.ReactElement;
                      const split = splitCommentPrefix(child.props?.children);
                      if (split) {
                        return (
                          <div className="comment-header">
                            <span className="comment-num">{split.num}</span>
                            <span className="comment-title">{split.rest}</span>
                          </div>
                        );
                      }
                    }
                    return <p {...props}>{children}</p>;
                  },
                }}
              >
                {normalizedMarkdown}
                </ReactMarkdown>
              </>
            )}
          </MathErrorBoundary>
        </div>
      </div>
    </div>
  );
}
