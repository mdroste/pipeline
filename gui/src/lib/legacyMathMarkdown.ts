/** Source-position edits protect existing math, links and code. This pass runs
 * before the rendering parse, so inserted delimiters cannot alter Markdown
 * structure. Recognition uses raw source offsets so Markdown escapes elsewhere
 * in a paragraph cannot suppress its math. */
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import {
  normalizeMathDelimiters,
  stripPresentationalHtml,
} from "./mathMarkdown";
import { stripInternalReportMarkers } from "./reportPresentation";
import { legacyMathSpans } from "./legacyMath";

interface Node {
  type: string;
  value?: string;
  position?: { start: { offset?: number }; end: { offset?: number } };
  children?: Node[];
}
const parser = unified().use(remarkParse).use(remarkGfm).use(remarkMath);
const PROTECTED = new Set([
  "code",
  "inlineCode",
  "math",
  "inlineMath",
  "link",
  "linkReference",
  "image",
  "imageReference",
  "html",
  "definition",
]);

export function promoteLegacyMath(markdown: string): string {
  // Avoid another parse for the ordinary prose and explicitly delimited case.
  if (!/[Α-Ωα-ω\u0302\u0303_^⁰¹²³⁴⁵⁶⁷⁸⁹ᴸîŶ]/u.test(markdown)) return markdown;
  const edits: Array<{ start: number; end: number; text: string }> = [];
  function visit(node: Node) {
    if (PROTECTED.has(node.type)) return;
    if (node.type === "text") {
      const start = node.position?.start.offset;
      const end = node.position?.end.offset;
      if (start === undefined || end === undefined) return;
      const source = markdown.slice(start, end);
      for (const span of legacyMathSpans(source)) {
        edits.push({
          start: start + span.start,
          end: start + span.end,
          text: `${markdown[start + span.start - 1] === "$" ? "\u2060" : ""}$${span.latex}$${markdown[start + span.end] === "$" ? "\u2060" : ""}`,
        });
      }
    } else node.children?.forEach(visit);
  }
  visit(parser.parse(markdown));
  let output = "";
  let cursor = 0;
  for (const edit of edits) {
    output += markdown.slice(cursor, edit.start) + edit.text;
    cursor = edit.end;
  }
  return output + markdown.slice(cursor);
}

/** Shared on-screen preparation; the source supplied by the run store is immutable. */
export function prepareReportMarkdown(markdown: string): string {
  return promoteLegacyMath(
    normalizeMathDelimiters(
      stripPresentationalHtml(stripInternalReportMarkers(markdown)),
    ),
  );
}
