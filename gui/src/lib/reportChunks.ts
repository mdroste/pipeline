import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

export const REPORT_CHUNK_CHARS = 80_000;
export interface ReportChunk {
  title: string;
  markdown: string;
  plain: boolean;
}
/** Split at Markdown block boundaries. Oversized single blocks remain readable
 * as paged source text; never cut a code fence and render its contents as prose. */
export function splitReport(
  markdown: string,
  limit = REPORT_CHUNK_CHARS,
): ReportChunk[] {
  if (markdown.length <= limit)
    return [{ title: "Report", markdown, plain: false }];
  const tree = unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkMath)
    .parse(markdown);
  const chunks: ReportChunk[] = [];
  let start = 0;
  let end = 0;
  let title = "Report";
  const definitions = tree.children
    .filter(
      (node) =>
        node.type === "definition" || node.type === "footnoteDefinition",
    )
    .map((node) =>
      markdown.slice(node.position?.start.offset, node.position?.end.offset),
    )
    .join("\n\n");
  const push = () => {
    if (end > start)
      chunks.push({
        title,
        markdown: markdown.slice(start, end),
        plain: false,
      });
    start = end;
  };
  for (const node of tree.children) {
    const nodeStart = node.position?.start.offset ?? end;
    const nodeEnd = node.position?.end.offset ?? nodeStart;
    if (nodeEnd - start > limit && end > start) push();
    if (node.type === "heading" && (end === start || node.depth <= 2)) {
      push();
      title = markdown
        .slice(nodeStart, nodeEnd)
        .replace(/^#+\s*/, "")
        .replace(/\s*#+\s*$/, "");
    }
    if (nodeEnd - start > limit) {
      for (let offset = start; offset < nodeEnd; offset += limit) {
        chunks.push({
          title: title + " · continued",
          markdown: markdown.slice(offset, Math.min(offset + limit, nodeEnd)),
          plain: true,
        });
      }
      start = nodeEnd;
    }
    end = nodeEnd;
  }
  end = markdown.length;
  push();
  // Definitions are non-rendering metadata and can be referenced from any part.
  return chunks.map((chunk) => ({
    ...chunk,
    markdown:
      chunk.markdown +
      (!chunk.plain && definitions ? "\n\n" + definitions : ""),
  }));
}
