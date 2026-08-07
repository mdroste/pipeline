import React, { memo, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import remarkParse from "remark-parse";
import { unified } from "unified";
import { useFindBar } from "../hooks/useFindBar";
import {
  normalizeMathDelimiters,
  stripPresentationalHtml,
} from "../lib/mathMarkdown";
import {
  rehypeValidateMath,
  remarkRepairMath,
  REPORT_KATEX_OPTIONS,
} from "../lib/mathRepair";
import ResizeHandle from "./ResizeHandle";
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import SafeMarkdownLink from "./SafeMarkdownLink";

interface Props {
  markdown: string;
}

interface MarkdownAstNode {
  type: string;
  value?: string;
  alt?: string;
  depth?: number;
  children?: MarkdownAstNode[];
  data?: {
    hProperties?: Record<string, unknown>;
    [key: string]: unknown;
  };
}

interface ReportHeading {
  level: number;
  text: string;
  id: string;
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

function slugBase(text: string): string {
  return (
    text
      .normalize("NFKC")
      .toLowerCase()
      .replace(/[^\p{Letter}\p{Number}\p{Mark}]+/gu, "-")
      .replace(/(^-+|-+$)/g, "") || "section"
  );
}

function astText(node: MarkdownAstNode): string {
  if (node.type === "image") return node.alt ?? "";
  if (typeof node.value === "string") return node.value;
  if (node.type === "break") return " ";
  return node.children?.map(astText).join("") ?? "";
}

function visitAst(
  node: MarkdownAstNode,
  visitor: (node: MarkdownAstNode) => void,
) {
  visitor(node);
  node.children?.forEach((child) => visitAst(child, visitor));
}

/**
 * Parse the document once to establish heading labels and IDs. A global used-ID
 * set avoids collisions such as "Intro", a duplicate "Intro", and "Intro-2".
 */
function buildHeadingIndex(markdown: string): ReportHeading[] {
  const tree = unified().use(remarkParse).use(remarkGfm).parse(markdown);
  const headings: ReportHeading[] = [];
  const usedIds = new Set<string>();

  visitAst(tree as MarkdownAstNode, (node) => {
    if (node.type !== "heading" || typeof node.depth !== "number") return;

    const text = astText(node).replace(/\s+/g, " ").trim();
    const base = slugBase(text);
    let id = base;
    let suffix = 2;
    while (usedIds.has(id)) {
      id = `${base}-${suffix}`;
      suffix += 1;
    }
    usedIds.add(id);
    headings.push({ level: node.depth, text, id });
  });

  return headings;
}

/**
 * Apply IDs from the same parsed heading index to the tree ReactMarkdown will
 * render. Sequence is stable because remark plugins do not add/remove headings.
 */
function headingIdPlugin(headings: readonly ReportHeading[]) {
  return () => (tree: unknown) => {
    let index = 0;
    visitAst(tree as MarkdownAstNode, (node) => {
      if (node.type !== "heading") return;
      const heading = headings[index];
      index += 1;
      if (!heading) return;
      node.data ??= {};
      node.data.hProperties = {
        ...node.data.hProperties,
        id: heading.id,
      };
    });
  };
}

/**
 * Reduce an inline Markdown heading to the label a reader expects in the TOC.
 * Report headings can contain model-authored HTML and Markdown decoration, but
 * the navigation should always be compact plain text.
 */
export function plainHeadingText(markdown: string): string {
  const protectedText: string[] = [];
  const protect = (value: string) => {
    const token = `\uE000${protectedText.length}\uE001`;
    protectedText.push(value);
    return token;
  };

  let text = markdown
    .trim()
    .replace(/\s+#+\s*$/, "")
    // Preserve literal formatting characters in code spans and escapes.
    .replace(/(`+)(.*?)\1/g, (_match, _ticks, value: string) => protect(value))
    .replace(
      /\\([\\`*_[\]{}()#+\-.!<>])/g,
      (_match, value: string) => protect(value),
    )
    .replace(/<!--.*?-->/g, "")
    .replace(/<\/?[A-Za-z][A-Za-z0-9-]*(?:\s[^<>]*?)?\s*\/?>/g, "")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\[[^\]]*\]/g, "$1");

  // Remove paired strong/emphasis/strikethrough delimiters, including nesting.
  let previous = "";
  while (text !== previous) {
    previous = text;
    text = text
      .replace(/(\*\*|__|~~)(.+?)\1/g, "$2")
      .replace(/(^|[\s([{"'])([*_])(?=\S)(.+?\S)\2(?=$|[\s)\]}"'.,!?;:])/g, "$1$3");
  }

  if (typeof document !== "undefined" && text.includes("&")) {
    const decoder = document.createElement("textarea");
    decoder.innerHTML = text;
    text = decoder.value;
  }

  return text
    .replace(/\uE000(\d+)\uE001/g, (_match, index: string) => {
      return protectedText[Number(index)] ?? "";
    })
    .replace(/\s+/g, " ")
    .trim();
}

/** Remove Pipeline's private markdown boundary tokens before presentation. */
export function stripInternalReportMarkers(markdown: string): string {
  return markdown.replace(
    /<!--\s*PIPELINE RUN DETAILS (?:START|END)\s*-->/gi,
    "",
  );
}

function ReportViewer({ markdown }: Props) {
  const contentRef = useRef<HTMLDivElement>(null);
  const [contentsOpen, setContentsOpen] = useState(true);
  const [contentsWidth, setContentsWidth] = usePersistentPanelWidth(
    "pipeline.ui.reportContentsWidth",
    240,
    184,
    360,
  );
  const normalizedMarkdown = useMemo(
    () =>
      normalizeMathDelimiters(
        stripPresentationalHtml(stripInternalReportMarkers(markdown)),
      ),
    [markdown],
  );
  const find = useFindBar(contentRef, normalizedMarkdown);

  const headingIndex = useMemo(
    () => buildHeadingIndex(normalizedMarkdown),
    [normalizedMarkdown],
  );
  const headings = useMemo(() => {
    return headingIndex.filter((heading) => heading.level <= 3);
  }, [headingIndex]);
  const remarkHeadingIds = useMemo(() => {
    return headingIdPlugin(headingIndex);
  }, [headingIndex]);

  return (
    <div className="flex h-full relative">
      {find.open && (
        <div className="absolute top-2 right-3 z-20 flex items-center gap-1 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg shadow-lg px-2 py-1">
          <input
            autoFocus
            aria-label="Find in report"
            value={find.query}
            onChange={(e) => find.setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); e.shiftKey ? find.prev() : find.next(); }
              if (e.key === "Escape") find.close();
            }}
            placeholder="Find in report…"
            className="w-44 py-0.5 px-1.5 text-sm bg-transparent text-gray-900 dark:text-gray-100 focus:outline-none"
          />
          <span className="text-xs text-gray-500 dark:text-gray-400 tabular-nums min-w-[3rem] text-right">
            {find.count > 0 ? `${find.current + 1}/${find.count}` : "0/0"}
          </span>
          <button aria-label="Previous match" onClick={find.prev} disabled={find.count === 0} className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100 disabled:opacity-30" title="Previous (Shift+Enter)">↑</button>
          <button aria-label="Next match" onClick={find.next} disabled={find.count === 0} className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100 disabled:opacity-30" title="Next (Enter)">↓</button>
          <button aria-label="Close find" onClick={find.close} className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100" title="Close (Esc)">✕</button>
        </div>
      )}
      {/* Table of contents */}
      {headings.length > 3 && contentsOpen && (
        <nav className="toc-nav relative" style={{ width: contentsWidth }}>
          <div className="mb-4 flex items-center justify-between gap-2">
            <h4 className="text-[11px] font-semibold uppercase tracking-widest text-gray-600 dark:text-gray-400">
              Contents
            </h4>
            <button
              type="button"
              onClick={() => setContentsOpen(false)}
              aria-label="Hide table of contents"
              className="rounded p-1 text-gray-500 hover:bg-gray-100 hover:text-gray-800
                         focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400
                         dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-100"
            >
              <svg aria-hidden="true" className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.8}>
                <path strokeLinecap="round" strokeLinejoin="round" d="m9 6 6 6-6 6" />
              </svg>
            </button>
          </div>
          <ul className="space-y-0.5">
            {headings
              .filter((h) => h.level <= 2)
              .map((h) => (
                <li key={h.id}>
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
          <ResizeHandle
            currentWidth={contentsWidth}
            defaultWidth={240}
            label="Resize table of contents"
            min={184}
            max={360}
            onResize={setContentsWidth}
          />
        </nav>
      )}
      {headings.length > 3 && !contentsOpen && (
        <button
          type="button"
          onClick={() => setContentsOpen(true)}
          aria-label="Show table of contents"
          className="hidden w-10 shrink-0 items-start justify-center border-r border-gray-200 pt-5 text-gray-500
                     hover:bg-gray-50 hover:text-gray-800 focus-visible:outline-none focus-visible:ring-2
                     focus-visible:ring-inset focus-visible:ring-gray-400 dark:border-gray-800 dark:hover:bg-gray-900
                     dark:text-gray-400 dark:hover:text-gray-100 lg:flex"
        >
          <svg aria-hidden="true" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.7}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M8 6h11M8 12h11M8 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01" />
          </svg>
        </button>
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
                  remarkPlugins={
                    fallback
                      ? [remarkGfm, remarkHeadingIds]
                      : [
                          remarkGfm,
                          remarkMath,
                          remarkRepairMath,
                          remarkHeadingIds,
                        ]
                  }
                  rehypePlugins={
                    fallback
                      ? []
                      : [
                          rehypeValidateMath,
                          [rehypeKatex, REPORT_KATEX_OPTIONS],
                        ]
                  }
                  components={{
                    a: SafeMarkdownLink,
                    table: ({ node: _node, ...props }) => (
                      <div className="max-w-full overflow-x-auto">
                        <table {...props} />
                      </div>
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

export default memo(ReportViewer);
