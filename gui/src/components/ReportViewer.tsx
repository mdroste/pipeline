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
  latexToReadableText,
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
  kind: "heading";
  level: number;
  text: string;
  id: string;
  hasProse: boolean;
}

interface ReportIssue {
  kind: "issue";
  level: 0;
  number: string;
  text: string;
  id: string;
}

type ReportNavigationEntry = ReportHeading | ReportIssue;

const MAX_RENDER_CHARS = 2_000_000;

class ReportRenderErrorBoundary extends React.Component<
  { markdown: string; children: React.ReactNode },
  { hasError: boolean }
> {
  state = { hasError: false };

  static getDerivedStateFromError() {
    return { hasError: true };
  }

  componentDidCatch(error: unknown) {
    console.error("Report rendering failed, falling back to plain text:", error);
  }

  componentDidUpdate(previous: { markdown: string }) {
    if (previous.markdown !== this.props.markdown && this.state.hasError) {
      this.setState({ hasError: false });
    }
  }

  render() {
    if (!this.state.hasError) return this.props.children;
    const truncated = this.props.markdown.length > MAX_RENDER_CHARS;
    return (
      <div className="h-full overflow-auto p-6">
        <div role="alert" className="mb-4 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
          This response could not be rendered as Markdown. Showing its preserved plain text instead.
        </div>
        <pre className="whitespace-pre-wrap break-words text-xs leading-relaxed text-gray-800 dark:text-gray-200">
          {this.props.markdown.slice(0, MAX_RENDER_CHARS)}
          {truncated ? "\n\n[Plain-text preview truncated.]" : ""}
        </pre>
      </div>
    );
  }
}

function isAuthorFootnoteMath(expression: string): boolean {
  return /^(?:\^\s*\{\s*(?:\*+|\\(?:ast|star|dagger|ddagger)|[\u2020\u2021])\s*\}|(?:\*+|\\(?:ast|star|dagger|ddagger)|[\u2020\u2021]))$/iu.test(
    expression.trim(),
  );
}

function hasNavigationProse(text: string): boolean {
  const withoutCommands = text.replace(/\\[A-Za-z]+/g, "");
  if (/\p{Letter}{2,}/u.test(withoutCommands)) return true;
  const hasFormulaSyntax = /[\\_^=<>+*/]/.test(withoutCommands);
  return !hasFormulaSyntax && /[\p{Letter}\p{Number}]/u.test(withoutCommands);
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

function astText(node: MarkdownAstNode, omitMath = false): string {
  if (node.type === "image") return node.alt ?? "";
  if (node.type === "inlineMath" || node.type === "math") {
    const expression = node.value ?? "";
    return omitMath || isAuthorFootnoteMath(expression)
      ? ""
      : latexToReadableText(expression);
  }
  if (typeof node.value === "string") return node.value;
  if (node.type === "break") return " ";
  return node.children?.map((child) => astText(child, omitMath)).join("") ?? "";
}

function visitAst(
  node: MarkdownAstNode,
  visitor: (node: MarkdownAstNode) => void,
) {
  visitor(node);
  node.children?.forEach((child) => visitAst(child, visitor));
}

function issueFromParagraph(node: MarkdownAstNode): { number: string; text: string } | null {
  if (node.type !== "paragraph" || node.children?.length !== 1) return null;
  const strong = node.children[0];
  if (strong.type !== "strong") return null;
  const match = plainHeadingText(astText(strong)).match(/^#(\d+)\.\s*(.+)$/);
  return match ? { number: match[1], text: match[2].trim() } : null;
}

/**
 * Parse the document once to establish navigation labels, IDs, and document
 * order. Markdown headings provide the section structure, while numbered issue
 * cards add direct links to the generated comments within those sections.
 */
function buildNavigationIndex(markdown: string): {
  headings: ReportHeading[];
  issues: ReportIssue[];
  entries: ReportNavigationEntry[];
} {
  const processor = unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkMath)
    .use(remarkRepairMath);
  const tree = processor.runSync(processor.parse(markdown));
  const headings: ReportHeading[] = [];
  const issues: ReportIssue[] = [];
  const entries: ReportNavigationEntry[] = [];
  const usedIds = new Set<string>();

  const uniqueId = (base: string) => {
    let id = base;
    let suffix = 2;
    while (usedIds.has(id)) {
      id = `${base}-${suffix}`;
      suffix += 1;
    }
    usedIds.add(id);
    return id;
  };

  visitAst(tree as MarkdownAstNode, (node) => {
    if (node.type === "heading" && typeof node.depth === "number") {
      const text = plainHeadingText(astText(node));
      const prose = plainHeadingText(astText(node, true));
      const heading: ReportHeading = {
        kind: "heading",
        level: node.depth,
        text,
        id: uniqueId(slugBase(text)),
        hasProse: hasNavigationProse(prose),
      };
      headings.push(heading);
      entries.push(heading);
      return;
    }
    const issue = issueFromParagraph(node);
    if (issue) {
      const entry: ReportIssue = {
        kind: "issue",
        level: 0,
        number: issue.number,
        text: issue.text,
        id: uniqueId(`issue-${issue.number}-${slugBase(issue.text)}`),
      };
      issues.push(entry);
      entries.push(entry);
    }
  });

  return { headings, issues, entries };
}

/**
 * Apply IDs from the same parsed navigation index to the tree ReactMarkdown
 * will render. Math repair can change text children but not the heading/issue
 * sequence used here.
 */
function navigationIdPlugin(
  headings: readonly ReportHeading[],
  issues: readonly ReportIssue[],
) {
  return () => (tree: unknown) => {
    let headingIndex = 0;
    let issueIndex = 0;
    visitAst(tree as MarkdownAstNode, (node) => {
      let entry: ReportNavigationEntry | undefined;
      if (node.type === "heading") {
        entry = headings[headingIndex];
        headingIndex += 1;
      } else if (issueFromParagraph(node)) {
        entry = issues[issueIndex];
        issueIndex += 1;
      }
      if (!entry) return;
      node.data ??= {};
      node.data.hProperties = {
        ...node.data.hProperties,
        id: entry.id,
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

  // TOC labels are plain text, so leaving inline math delimiters untouched
  // exposes model-authored TeX such as `$\\phi$`. Reuse the report math
  // fallback's compact text representation to preserve meaningful symbols
  // without leaking raw delimiters, commands, or braces into navigation.
  // PaddleOCR author-footnote markers remain useful in the document title but
  // are not part of its navigation label, so omit those entirely.
  text = text.replace(
    /(\$\$|\$)([\s\S]*?)\1/g,
    (_match, _delimiter: string, expression: string) =>
      isAuthorFootnoteMath(expression) ? "" : latexToReadableText(expression),
  );

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

function ReportViewerContent({ markdown }: Props) {
  const contentRef = useRef<HTMLDivElement>(null);
  const [contentsOpen, setContentsOpen] = useState(true);
  const [contentsWidth, setContentsWidth] = usePersistentPanelWidth(
    "pipeline.ui.reportContentsWidth",
    240,
    184,
    360,
  );
  const previewTruncated = markdown.length > MAX_RENDER_CHARS;
  const boundedMarkdown = useMemo(
    () => markdown.slice(0, MAX_RENDER_CHARS),
    [markdown],
  );
  const normalizedMarkdown = useMemo(
    () =>
      normalizeMathDelimiters(
        stripPresentationalHtml(stripInternalReportMarkers(boundedMarkdown)),
      ),
    [boundedMarkdown],
  );
  const find = useFindBar(contentRef, normalizedMarkdown);

  const navigationIndex = useMemo(
    () => buildNavigationIndex(normalizedMarkdown),
    [normalizedMarkdown],
  );
  const headings = useMemo(() => {
    return navigationIndex.headings.filter(
      (heading) => heading.level <= 3 && heading.hasProse,
    );
  }, [navigationIndex.headings]);
  const contents = useMemo<ReportNavigationEntry[]>(() => {
    if (navigationIndex.issues.length) {
      return navigationIndex.entries.filter(
        (entry) =>
          entry.kind === "issue" || (entry.level <= 2 && entry.hasProse),
      );
    }
    return headings.length > 3
      ? headings.filter((heading) => heading.level <= 2)
      : [];
  }, [headings, navigationIndex.entries, navigationIndex.issues.length]);
  const remarkNavigationIds = useMemo(() => {
    return navigationIdPlugin(navigationIndex.headings, navigationIndex.issues);
  }, [navigationIndex.headings, navigationIndex.issues]);

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
      {contents.length > 0 && contentsOpen && (
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
            {contents.map((entry) => (
                <li key={entry.id}>
                  <a
                    href={`#${entry.id}`}
                    className={`toc-link ${
                      entry.kind === "issue"
                        ? "toc-issue"
                        : entry.level === 1 ? "toc-h1" : "toc-h2"
                    }`}
                  >
                    {entry.kind === "issue" && (
                      <span className="toc-issue-number">#{entry.number}</span>
                    )}
                    <span>{entry.text}</span>
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
      {contents.length > 0 && !contentsOpen && (
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
          {previewTruncated && (
            <div className="mb-4 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
              This unusually large response is truncated in the interactive preview. The saved artifact remains unchanged.
            </div>
          )}
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
                      ? [remarkGfm, remarkNavigationIds]
                      : [
                          remarkGfm,
                          remarkMath,
                          remarkRepairMath,
                          remarkNavigationIds,
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
                        const child = childArray[0] as React.ReactElement<{
                          children?: React.ReactNode;
                        }>;
                        const split = splitCommentPrefix(child.props?.children);
                        if (split) {
                          return (
                            <div {...props} className="comment-header">
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

function ReportViewer(props: Props) {
  return (
    <ReportRenderErrorBoundary markdown={props.markdown}>
      <ReportViewerContent {...props} />
    </ReportRenderErrorBoundary>
  );
}

export default memo(ReportViewer);
