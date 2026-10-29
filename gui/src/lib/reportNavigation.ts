import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { unified } from "unified";
import { latexToReadableText, remarkRepairMath } from "./mathRepair";

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

export type ReportNavigationEntry = ReportHeading | ReportIssue;

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

function issueFromParagraph(
  node: MarkdownAstNode,
): { number: string; text: string } | null {
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
export function buildNavigationIndex(markdown: string): {
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
export function navigationIdPlugin(
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
    .replace(/\\([\\`*_[\]{}()#+\-.!<>])/g, (_match, value: string) =>
      protect(value),
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
      .replace(
        /(^|[\s([{"'])([*_])(?=\S)(.+?\S)\2(?=$|[\s)\]}"'.,!?;:])/g,
        "$1$3",
      );
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
