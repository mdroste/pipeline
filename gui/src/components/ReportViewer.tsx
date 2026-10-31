import {
  buildNavigationIndex,
  navigationIdPlugin,
  type ReportNavigationEntry,
} from "../lib/reportNavigation";
import { stripInternalReportMarkers } from "../lib/reportPresentation";
import {
  ReportRenderErrorBoundary,
  MathErrorBoundary,
} from "./report-viewer/RenderBoundaries";
import { MAX_RENDER_CHARS } from "./report-viewer/limits";
import ReportContents from "./report-viewer/ReportContents";
import { useAppPreferences } from "../lib/appPreferences";
import useContainerWidth from "../hooks/useContainerWidth";
import React, {
  memo,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { reportAnchorPlugin } from "../lib/reportAnchors";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  fileMarkdownComponents,
  fileMarkdownUrlTransform,
} from "./file-workspace/markdownComponents";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
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
import usePersistentPanelWidth from "../hooks/usePersistentPanelWidth";
import SafeMarkdownLink from "./SafeMarkdownLink";

interface Props {
  markdown: string;
  initialContentsOpen?: boolean;
  /** Scrolls to the heading with this text; a new key repeats the request. */
  scrollRequest?: { key: number; heading: string } | null;
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

/** Hoisted so the memoized markdown subtree receives a stable reference. */
const MARKDOWN_COMPONENTS: Components = {
  ...fileMarkdownComponents,
  a: SafeMarkdownLink,
  table: ({ node: _node, ...props }) => (
    <div className="max-w-full overflow-x-auto">
      <table {...props} />
    </div>
  ),
  // Detect comment headers: a paragraph whose only child is
  // <strong>#N. Title</strong>  →  render as a styled card.
  p: ({ children, node: _node, ...props }) => {
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
};

/**
 * The parsed-and-rendered report body. Memoized on the normalized markdown
 * string (plus the stable navigation plugin derived from it) so find-bar
 * keystrokes in the parent never re-run the markdown/KaTeX pipeline — on
 * large math-heavy reports that re-parse takes seconds per keystroke.
 */
const ReportMarkdown = memo(function ReportMarkdown({
  markdown,
  remarkNavigationIds,
  rehypeAnchors,
}: {
  markdown: string;
  remarkNavigationIds: ReturnType<typeof navigationIdPlugin>;
  rehypeAnchors: ReturnType<typeof reportAnchorPlugin>;
}) {
  return (
    <MathErrorBoundary resetKey={markdown}>
      {(fallback) => (
        <>
          {fallback && (
            <div className="mb-4 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
              Some math in this report could not be rendered — formulas are
              shown as raw LaTeX notation.
            </div>
          )}
          <ReactMarkdown
            urlTransform={fileMarkdownUrlTransform}
            remarkPlugins={
              fallback
                ? [remarkGfm, remarkNavigationIds]
                : [remarkGfm, remarkMath, remarkRepairMath, remarkNavigationIds]
            }
            rehypePlugins={
              fallback
                ? [rehypeAnchors]
                : [
                    rehypeValidateMath,
                    [rehypeKatex, REPORT_KATEX_OPTIONS],
                    rehypeAnchors,
                  ]
            }
            components={MARKDOWN_COMPONENTS}
          >
            {markdown}
          </ReactMarkdown>
        </>
      )}
    </MathErrorBoundary>
  );
});

function ReportViewerContent({
  markdown,
  initialContentsOpen = true,
  scrollRequest = null,
}: Props) {
  const contentRef = useRef<HTMLDivElement>(null);
  const instanceId = useId();
  const anchorPrefix = `report-${instanceId.replace(/[^a-zA-Z0-9_-]/g, "")}-`;
  const rehypeAnchors = useMemo(
    () => reportAnchorPlugin(anchorPrefix),
    [anchorPrefix],
  );
  const [activeAnchor, setActiveAnchor] = useState<string | null>(null);
  const [contentsOpen, setContentsOpen] = useState(initialContentsOpen);
  const [readerRef, readerWidth] = useContainerWidth<HTMLDivElement>();
  const compact = readerWidth !== null && readerWidth < 760;
  const [compactContentsOpen, setCompactContentsOpen] = useState(false);
  const outlineVisible = compact ? compactContentsOpen : contentsOpen;
  const contentsToggle = useRef<HTMLButtonElement>(null);
  const hideContents = () => {
    if (compact) setCompactContentsOpen(false);
    else setContentsOpen(false);
    requestAnimationFrame(() => contentsToggle.current?.focus());
  };
  const outlineId = `${instanceId}-contents`;
  const { reportScale } = useAppPreferences();
  const [fontOverride, setFontScale] = useState<number | null>(null);
  const fontScale = fontOverride ?? reportScale;
  const [wideReading, setWideReading] = useState(false);
  const [relaxedLeading, setRelaxedLeading] = useState(true);
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

  const requestedHeading = scrollRequest?.heading;
  const requestKey = scrollRequest?.key;
  useEffect(() => {
    if (!requestedHeading) return;
    const target = Array.from(
      contentRef.current?.querySelectorAll<HTMLElement>("h1, h2, h3") ?? [],
    ).find((heading) => heading.textContent?.trim() === requestedHeading);
    if (!target) return;
    setActiveAnchor(target.id || null);
    target.scrollIntoView({ block: "start" });
  }, [requestKey, requestedHeading]);

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
    <div
      ref={readerRef}
      className={`report-reader flex h-full min-h-0 min-w-0 relative ${compact ? "reader-compact flex-col" : ""}`}
      onClickCapture={(event) => {
        const link =
          event.target instanceof Element
            ? event.target.closest("a[href^='#']")
            : null;
        const href = link?.getAttribute("href");
        if (!href) return;
        // Never let a missing fragment navigate another reader or the app shell.
        event.preventDefault();
        let id: string;
        try {
          id = decodeURIComponent(href.slice(1));
        } catch {
          return;
        }
        const target = Array.from(
          contentRef.current?.querySelectorAll<HTMLElement>("[id]") ?? [],
        ).find((node) => node.id === id);
        if (!target) return;
        setActiveAnchor(id);
        if (compact) setCompactContentsOpen(false);
        target.scrollIntoView({ block: "start" });
        if (!target.hasAttribute("tabindex")) target.tabIndex = -1;
        target.focus({ preventScroll: true });
      }}
    >
      {find.open && (
        <div className="absolute top-2 right-3 z-20 flex items-center gap-1 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg shadow-lg px-2 py-1">
          <input
            autoFocus
            aria-label="Find in report"
            value={find.query}
            onChange={(e) => find.setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                e.shiftKey ? find.prev() : find.next();
              }
              if (e.key === "Escape") find.close();
            }}
            placeholder="Find in report…"
            className="w-44 py-0.5 px-1.5 text-sm bg-transparent text-gray-900 dark:text-gray-100 focus:outline-none"
          />
          <span className="text-xs text-gray-500 dark:text-gray-400 tabular-nums min-w-[3rem] text-right">
            {find.count > 0
              ? `${find.current + 1}/${find.count}${find.capped ? "+" : ""}`
              : "0/0"}
          </span>
          <button
            aria-label="Previous match"
            onClick={find.prev}
            disabled={find.count === 0}
            className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100 disabled:opacity-30"
            title="Previous (Shift+Enter)"
          >
            ↑
          </button>
          <button
            aria-label="Next match"
            onClick={find.next}
            disabled={find.count === 0}
            className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100 disabled:opacity-30"
            title="Next (Enter)"
          >
            ↓
          </button>
          <button
            aria-label="Close find"
            onClick={find.close}
            className="px-1 text-gray-500 hover:text-gray-900 dark:hover:text-gray-100"
            title="Close (Esc)"
          >
            ✕
          </button>
        </div>
      )}
      <ReportContents
        compact={compact}
        contents={contents}
        contentsToggle={contentsToggle}
        outlineVisible={outlineVisible}
        outlineId={outlineId}
        setCompactContentsOpen={setCompactContentsOpen}
        contentsWidth={contentsWidth}
        hideContents={hideContents}
        anchorPrefix={anchorPrefix}
        activeAnchor={activeAnchor}
        setContentsWidth={setContentsWidth}
        contentsOpen={contentsOpen}
        setContentsOpen={setContentsOpen}
      />

      {/* Report content */}
      <div className="relative min-h-0 min-w-0 flex-1 overflow-y-auto">
        <div className="sticky right-3 top-2 z-10 ml-auto mr-3 flex max-w-full flex-wrap w-fit items-center gap-1 rounded-lg border border-gray-200 bg-white/95 p-1 text-xs shadow-sm backdrop-blur dark:border-gray-700 dark:bg-gray-900/95">
          <button
            aria-label="Decrease report text size"
            onClick={() =>
              setFontScale((value) => Math.max(85, (value ?? reportScale) - 5))
            }
            className="rounded px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-800"
          >
            A−
          </button>
          <button
            aria-label="Reset report text size"
            onClick={() => setFontScale(null)}
            className="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"
          >
            {fontScale}%
          </button>
          <button
            aria-label="Increase report text size"
            onClick={() =>
              setFontScale((value) => Math.min(150, (value ?? reportScale) + 5))
            }
            className="rounded px-2 py-1 hover:bg-gray-100 dark:hover:bg-gray-800"
          >
            A+
          </button>
          <button
            aria-pressed={wideReading}
            onClick={() => setWideReading((value) => !value)}
            className="rounded border-l border-gray-200 px-2 py-1 text-gray-500 hover:bg-gray-100 dark:border-gray-700 dark:hover:bg-gray-800"
          >
            {wideReading ? "Comfort width" : "Wide text"}
          </button>
          <button
            aria-pressed={relaxedLeading}
            onClick={() => setRelaxedLeading((value) => !value)}
            className="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800"
          >
            {relaxedLeading ? "Compact lines" : "Relaxed lines"}
          </button>
        </div>
        {/* Key on the content so a change fully remounts this subtree. The
            find bar mutates these DOM nodes (wrapping matches in <mark>); a
            remount lets React discard the mutated tree wholesale instead of
            diffing against it, which otherwise throws or shows stale text. */}
        <div
          key={normalizedMarkdown}
          className="report-content"
          ref={contentRef}
          style={{
            fontSize: `${fontScale}%`,
            lineHeight: relaxedLeading ? 1.75 : 1.5,
            maxWidth: wideReading ? "68rem" : "52rem",
          }}
        >
          {previewTruncated && (
            <div className="mb-4 rounded border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-700 dark:bg-amber-950 dark:text-amber-200">
              This response is too long to show in full. Open the saved file to
              read the rest.
            </div>
          )}
          <ReportMarkdown
            markdown={normalizedMarkdown}
            remarkNavigationIds={remarkNavigationIds}
            rehypeAnchors={rehypeAnchors}
          />
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
