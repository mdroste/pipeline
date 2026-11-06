import { memo, useMemo } from "react";
import ReactMarkdown, { type Components, type Options } from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import "katex/dist/katex.min.css";
import {
  mathOnlyMarkdown,
  normalizeMathDelimiters,
  stripPresentationalHtml,
} from "../../lib/mathMarkdown";
import {
  hasRepairableMath,
  rehypeValidateMath,
  remarkRepairMath,
  REPORT_KATEX_OPTIONS,
} from "../../lib/mathRepair";
import { stripInternalReportMarkers } from "../../lib/reportPresentation";
import SafeMarkdownLink from "../SafeMarkdownLink";
import { MathErrorBoundary } from "./RenderBoundaries";

const PLUGINS = [remarkGfm, remarkMath, remarkRepairMath];
const FALLBACK_PLUGINS = [remarkGfm];
const REHYPE_MATH: Options["rehypePlugins"] = [
  rehypeValidateMath,
  [rehypeKatex, REPORT_KATEX_OPTIONS],
];
const COMPONENTS: Components = {
  a: SafeMarkdownLink,
  table: ({ node: _node, ...props }) => (
    <div className="max-w-full overflow-x-auto">
      <table {...props} />
    </div>
  ),
};

/** Report text outside the Report tab, such as one finding: the same cleaning
 *  and math, without the contents list, anchors, or comment cards. Memoized so
 *  a parent's keystrokes never re-run the Markdown and KaTeX pipeline. */
const CompactMarkdown = memo(function CompactMarkdown({
  markdown,
}: {
  markdown: string;
}) {
  const cleaned = useMemo(
    () =>
      normalizeMathDelimiters(
        stripPresentationalHtml(stripInternalReportMarkers(markdown)),
      ),
    [markdown],
  );
  return (
    <MathErrorBoundary resetKey={cleaned}>
      {(fallback) => (
        <ReactMarkdown
          remarkPlugins={fallback ? FALLBACK_PLUGINS : PLUGINS}
          rehypePlugins={fallback ? undefined : REHYPE_MATH}
          components={COMPONENTS}
        >
          {cleaned}
        </ReactMarkdown>
      )}
    </MathErrorBoundary>
  );
});

const INLINE_COMPONENTS: Components = {
  // GFM links a bare URL after parsing, and a label sits inside a button.
  a: ({ children }) => <>{children}</>,
  p: ({ children }) => <>{children}</>,
};

/** A title or quote: its math is typeset and the rest stays plain text. A
 *  label without math skips the Markdown and KaTeX pipeline. `verbatim` keeps
 *  text that only looks like a tag, because a quote is checked against its
 *  source. */
export const InlineMathText = memo(function InlineMathText({
  text,
  verbatim = false,
}: {
  text: string;
  verbatim?: boolean;
}) {
  const plain = useMemo(
    () =>
      (verbatim
        ? text
        : stripPresentationalHtml(stripInternalReportMarkers(text))
      )
        .replace(/\s+/g, " ")
        .trim(),
    [text, verbatim],
  );
  const markdown = useMemo(
    () => mathOnlyMarkdown(plain, !verbatim && hasRepairableMath(plain)),
    [plain, verbatim],
  );
  if (markdown === null) return <>{plain}</>;
  return (
    <MathErrorBoundary resetKey={markdown}>
      {(fallback) =>
        fallback ? (
          <>{plain}</>
        ) : (
          <ReactMarkdown
            remarkPlugins={PLUGINS}
            rehypePlugins={REHYPE_MATH}
            components={INLINE_COMPONENTS}
          >
            {markdown}
          </ReactMarkdown>
        )
      }
    </MathErrorBoundary>
  );
});

export default CompactMarkdown;
