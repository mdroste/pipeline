import { memo, useMemo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import "katex/dist/katex.min.css";
import {
  normalizeMathDelimiters,
  stripPresentationalHtml,
} from "../../lib/mathMarkdown";
import {
  rehypeValidateMath,
  remarkRepairMath,
  REPORT_KATEX_OPTIONS,
} from "../../lib/mathRepair";
import { stripInternalReportMarkers } from "../../lib/reportPresentation";
import SafeMarkdownLink from "../SafeMarkdownLink";
import { MathErrorBoundary } from "./RenderBoundaries";

const PLUGINS = [remarkGfm, remarkMath, remarkRepairMath];
const FALLBACK_PLUGINS = [remarkGfm];
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
          rehypePlugins={
            fallback
              ? undefined
              : [rehypeValidateMath, [rehypeKatex, REPORT_KATEX_OPTIONS]]
          }
          components={COMPONENTS}
        >
          {cleaned}
        </ReactMarkdown>
      )}
    </MathErrorBoundary>
  );
});

export default CompactMarkdown;
