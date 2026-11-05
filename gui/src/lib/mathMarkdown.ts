// One rule for `$` everywhere on screen. Two amounts are not a pair: a span that
// starts with a digit may not end in a space ("$5 to $8") or close before a
// digit ("$5-$8"). A sign after a word or number is a currency unless math
// follows it ("US$ and A$", against `Smith$^{1}$`). Inner spaces alone cannot
// decide this, because PDF extraction writes real math as `$ ^{*} $`, and math
// may wrap within a paragraph.
const DOLLAR_MATH_SOURCE = String.raw`(?<![\\$])\$(?!\$)(?!(?<=[A-Za-z0-9/]\$)(?:\s(?![\^_\\{])|[).,;:\]]))(?!\d(?:\\.|[^$\\])*(?:\s\$|\$\d))(?:\\.|[^$\\])+\$`;
const DOLLAR_MATH_AT = new RegExp(DOLLAR_MATH_SOURCE, "y");
const PLAIN_TEXT_MATH_RE = new RegExp(
  String.raw`\$\$[\s\S]*?\$\$|` + DOLLAR_MATH_SOURCE,
  "g",
);

/** Replace each math span in plain text. A currency `$` stays as written. */
export function replaceMathSpans(
  text: string,
  replace: (expression: string) => string,
): string {
  return text.replace(PLAIN_TEXT_MATH_RE, (span) => {
    const width = span.startsWith("$$") ? 2 : 1;
    return replace(span.slice(width, -width));
  });
}

function fenceMarker(line: string): { marker: string; count: number } | null {
  const trimmed = line.trimStart();
  const marker = trimmed[0];
  if (marker !== "`" && marker !== "~") return null;
  let count = 0;
  while (trimmed[count] === marker) count += 1;
  return count >= 3 ? { marker, count } : null;
}

const DISPLAY_ENVIRONMENTS: Record<string, { open: string; close: string }> = {
  equation: { open: "$$", close: "$$" },
  "equation*": { open: "$$", close: "$$" },
  displaymath: { open: "$$", close: "$$" },
  align: { open: "$$\n\\begin{aligned}", close: "\\end{aligned}\n$$" },
  "align*": { open: "$$\n\\begin{aligned}", close: "\\end{aligned}\n$$" },
  gather: { open: "$$\n\\begin{gathered}", close: "\\end{gathered}\n$$" },
  "gather*": { open: "$$\n\\begin{gathered}", close: "\\end{gathered}\n$$" },
};

function normalizeDisplayEnvironment(line: string): string | null {
  const newline = line.endsWith("\n") ? "\n" : "";
  const body = newline ? line.slice(0, -1) : line;
  const match = body.match(
    /^(\s*)\\(begin|end)\{(equation\*?|displaymath|align\*?|gather\*?)\}\s*$/,
  );
  if (!match) return null;
  const replacement =
    match[2] === "begin"
      ? DISPLAY_ENVIRONMENTS[match[3]].open
      : DISPLAY_ENVIRONMENTS[match[3]].close;
  return `${match[1]}${replacement}${newline}`;
}

const URL_AT = /<?(?:https?:\/\/|www\.)[^\s<>]+>?/y;
const INDENTED_CODE_RE = /^(?: {4}|\t)/;
const LIST_ITEM_RE = /^\s*(?:[-*+]|\d{1,9}[.)])\s/;
// Keeps `$x$` and a script right after it from reading as a `$$` delimiter.
const MATH_SEPARATOR = "\u2060";

/** Where a code span opened by `count` backticks closes, or -1 if it does not. */
function closingTicks(text: string, from: number, count: number): number {
  let index = from;
  while (index < text.length) {
    const start = text.indexOf("`", index);
    if (start < 0) return -1;
    let end = start;
    while (text[end] === "`") end += 1;
    if (end - start === count) return start;
    index = end;
  }
  return -1;
}

function normalizeParagraph(text: string): string {
  let output = "";
  let index = 0;
  while (index < text.length) {
    const char = text[index];
    if (char === "`") {
      let count = 1;
      while (text[index + count] === "`") count += 1;
      const close = closingTicks(text, index + count, count);
      // An unclosed run is plain text, as in CommonMark.
      const end = close < 0 ? index + count : close + count;
      output += text.slice(index, end);
      index = end;
      continue;
    }
    if (
      (char === "h" || char === "w" || char === "<") &&
      !/\w/.test(text[index - 1] ?? "")
    ) {
      URL_AT.lastIndex = index;
      const url = URL_AT.exec(text)?.[0];
      if (url) {
        output += url;
        index += url.length;
        continue;
      }
    }
    if (char === "\\" && text[index - 1] !== "\\" && index + 1 < text.length) {
      const next = text[index + 1];
      if (next === "(" || next === ")") {
        output += next === "(" && output.endsWith("$") ? MATH_SEPARATOR : "";
        output += "$";
        index += 2;
        continue;
      }
      if (next === "[" || next === "]") {
        output += "$$";
        index += 2;
        continue;
      }
    }
    if (char === "$" && text[index - 1] !== "\\") {
      if (text[index + 1] === "$") {
        const close = text.indexOf("$$", index + 2);
        const end = close < 0 ? index + 2 : close + 2;
        output += text.slice(index, end);
        index = end;
        continue;
      }
      DOLLAR_MATH_AT.lastIndex = index;
      const span = DOLLAR_MATH_AT.exec(text)?.[0];
      if (span && !span.includes("`")) {
        output += span;
        index += span.length;
        continue;
      }
      // remark-math pairs any two `$` in a paragraph, so a currency sign has
      // to be escaped to stay text.
      output += "\\$";
      index += 1;
      continue;
    }
    output += char;
    index += 1;
  }
  return output;
}

/** Normalize legacy LaTeX math delimiters without changing code examples. */
export function normalizeMathDelimiters(markdown: string): string {
  let fence: { marker: string; count: number } | null = null;
  let output = "";
  let paragraph = "";
  let afterBlank = true;
  let inList = false;
  let inIndentedCode = false;
  const finishParagraph = () => {
    output += normalizeParagraph(paragraph);
    paragraph = "";
  };
  for (const line of markdown.split(/(?<=\n)/)) {
    const marker = fenceMarker(line);
    if (marker || fence) {
      finishParagraph();
      if (marker) {
        if (
          fence &&
          marker.marker === fence.marker &&
          marker.count >= fence.count
        ) {
          fence = null;
        } else if (!fence) {
          fence = marker;
        }
      }
      output += line;
      afterBlank = false;
      continue;
    }
    if (!line.trim()) {
      finishParagraph();
      output += line;
      afterBlank = true;
      continue;
    }
    if (LIST_ITEM_RE.test(line)) inList = true;
    else if (afterBlank && !/^\s/.test(line)) inList = false;
    inIndentedCode =
      INDENTED_CODE_RE.test(line) && !inList && (afterBlank || inIndentedCode);
    afterBlank = false;
    const display = normalizeDisplayEnvironment(line);
    if (inIndentedCode || display !== null) {
      finishParagraph();
      output += display ?? line;
      continue;
    }
    paragraph += line;
  }
  finishParagraph();
  return output;
}

// The tag name must immediately follow `<` or `</` (CommonMark's inline-HTML
// rule): tolerating whitespace there made prose inequalities like
// "T < N and R > 1" match as a pseudo-tag and vanish from the rendered report.
const RAW_HTML_TAG_RE =
  /<\/?([A-Za-z][A-Za-z0-9-]*)((?:\s(?:[^<>"']|"[^"]*"|'[^']*')*)?)\/?>/g;
// Single letters such as `b` and `i` are element names as well as variables, so
// a tag's attributes must look like attributes: "a<b and c>d" has bare words.
const HTML_ATTRIBUTES_RE =
  /^(?:\s+(?:[A-Za-z_:][\w:.-]*\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'=<>`]+)|async|autofocus|autoplay|checked|controls|defer|disabled|download|hidden|loop|multiple|muted|nowrap|open|readonly|required|reversed|selected))*\s*\/?$/i;
const BLOCK_HTML_TAGS = new Set([
  "address",
  "article",
  "aside",
  "blockquote",
  "center",
  "dd",
  "div",
  "dl",
  "dt",
  "figcaption",
  "figure",
  "footer",
  "header",
  "li",
  "main",
  "nav",
  "ol",
  "p",
  "section",
  "table",
  "tbody",
  "td",
  "tfoot",
  "th",
  "thead",
  "tr",
  "ul",
]);

// Everything else a model may emit. Only known element names are tags: `<T>`,
// `<name>` and the `<` of an inequality are text.
const OTHER_HTML_TAGS = new Set([
  "a",
  "abbr",
  "acronym",
  "area",
  "audio",
  "b",
  "base",
  "bdi",
  "bdo",
  "big",
  "body",
  "button",
  "canvas",
  "caption",
  "cite",
  "code",
  "col",
  "colgroup",
  "data",
  "datalist",
  "del",
  "details",
  "dfn",
  "dialog",
  "em",
  "embed",
  "fieldset",
  "font",
  "form",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "head",
  "hgroup",
  "hr",
  "html",
  "i",
  "iframe",
  "img",
  "input",
  "ins",
  "kbd",
  "label",
  "legend",
  "link",
  "map",
  "mark",
  "menu",
  "meta",
  "meter",
  "nobr",
  "noscript",
  "object",
  "optgroup",
  "option",
  "output",
  "param",
  "picture",
  "pre",
  "progress",
  "q",
  "rp",
  "rt",
  "ruby",
  "s",
  "samp",
  "script",
  "select",
  "slot",
  "small",
  "source",
  "span",
  "strike",
  "strong",
  "style",
  "sub",
  "summary",
  "sup",
  "svg",
  "template",
  "textarea",
  "time",
  "title",
  "track",
  "tt",
  "u",
  "var",
  "video",
  "wbr",
]);

const SCRIPT_TAG_RE =
  /<(sup|sub)((?:\s(?:[^<>"']|"[^"]*"|'[^']*')*)?)>(.*?)<\/\1\s*>/gi;
const TEX_TEXT_ESCAPES: Record<string, string> = {
  "\\": "\\textbackslash{}",
  "{": "\\{",
  "}": "\\}",
  $: "\\textdollar{}",
  "%": "\\%",
  "&": "\\&",
  "#": "\\#",
  _: "\\_",
  "^": "\\textasciicircum{}",
  "~": "\\textasciitilde{}",
};

/** Whether a RAW_HTML_TAG_RE match is an element rather than prose. */
function isHtmlTag(tagName: string, attributes: string): boolean {
  const name = tagName.toLowerCase();
  return (
    (name === "br" || BLOCK_HTML_TAGS.has(name) || OTHER_HTML_TAGS.has(name)) &&
    HTML_ATTRIBUTES_RE.test(attributes)
  );
}

/** Drop HTML tags from a one-line label. "N<T and T>0" is prose and stays. */
export function removeHtmlTags(text: string): string {
  return text.replace(RAW_HTML_TAG_RE, (tag, tagName, attributes) =>
    isHtmlTag(tagName, attributes) ? "" : tag,
  );
}

/**
 * `10<sup>6</sup>` is not 106, so a script stays a script. The `\(` form is
 * written instead of `$` because normalizeMathDelimiters converts it without
 * pairing it against a currency sign earlier on the line. A pipe would split a
 * table cell, and links and emphasis mean nothing inside math, so that text
 * keeps the plain marker.
 */
function scriptAsMath(
  tag: string,
  name: string,
  attributes: string,
  inner: string,
): string {
  if (!HTML_ATTRIBUTES_RE.test(attributes)) return tag;
  const text = stripHtmlChunk(inner, true).trim();
  if (!text) return "";
  if (/[|[\]*`]/.test(text)) return tag;
  const body = /^(?:[\d\s+\-−.,*()]+|[A-Za-z])$/.test(text)
    ? text
    : `\\text{${text.replace(/[\\{}$%&#_^~]/g, (char) => TEX_TEXT_ESCAPES[char])}}`;
  return `\\(${name.toLowerCase() === "sup" ? "^" : "_"}{${body}}\\)`;
}

/** A table row must stay on one line, so its breaks become spaces. */
function stripHtmlChunk(chunk: string, tableRow: boolean): string {
  return (
    chunk
      .replace(SCRIPT_TAG_RE, (tag, name, attributes, inner) =>
        scriptAsMath(tag, name, attributes, inner),
      )
      // Written math is cut out before this runs, so these spans are all scripts.
      .replaceAll("\\)\\(", "{}")
      .replace(RAW_HTML_TAG_RE, (tag, tagName, attributes) => {
        if (!isHtmlTag(tagName, attributes)) return tag;
        const name = String(tagName).toLowerCase();
        if (name === "br" || BLOCK_HTML_TAGS.has(name))
          return tableRow ? " " : "\n";
        // A script whose content can't be math still reads as one.
        if ((name === "sup" || name === "sub") && !tag.startsWith("</"))
          return name === "sup" ? "^" : "_";
        return "";
      })
  );
}

// Math on one line, in every form normalizeMathDelimiters accepts, because the
// cleaner runs first. An escaped or unpaired `$` is text, as in remark-math.
const MATH_SPAN_RE = new RegExp(
  String.raw`(?<!\\)\$\$.+?\$\$|\\\[.+?\\\]|\\\(.+?\\\)|` + DOLLAR_MATH_SOURCE,
  "g",
);

/** Apply `plain` outside the math of one line and `math` to each math span. */
function mapMathSpans(
  text: string,
  plain: (text: string) => string,
  math: (span: string) => string,
): string {
  let output = "";
  let plainStart = 0;
  for (const span of text.matchAll(MATH_SPAN_RE)) {
    output += plain(text.slice(plainStart, span.index)) + math(span[0]);
    plainStart = span.index + span[0].length;
  }
  return output + plain(text.slice(plainStart));
}

// An even run of backslashes escapes itself, so its pipe still ends a cell.
const UNESCAPED_PIPE_RE = /(?<!\\)((?:\\\\)*)\|/g;

/**
 * Keep a pipe from ending a table cell. GFM unescapes `\|` in text but not in
 * math, where it would mean the norm bars, so math writes `\vert`.
 */
export function escapeTablePipes(text: string): string {
  return mapMathSpans(
    text,
    (plain) => plain.replace(UNESCAPED_PIPE_RE, "$1\\|"),
    (span) => span.replace(UNESCAPED_PIPE_RE, "$1\\vert "),
  );
}

const ASCII_PUNCTUATION_RE = /[!-/:-@[-`{-~]/g;

/**
 * Markdown for a one-line label whose math is typeset and whose other text
 * stays exactly as written, or null if it has no math. Character references
 * are text to Markdown, so a title cannot turn into a list or emphasis. A bare
 * URL still becomes a link, which `InlineMathText` renders as text.
 */
export function mathOnlyMarkdown(text: string): string | null {
  if (text.search(MATH_SPAN_RE) < 0) return null;
  return normalizeMathDelimiters(
    mapMathSpans(
      text,
      (plain) =>
        plain.replace(
          ASCII_PUNCTUATION_RE,
          (char) => `&#${char.charCodeAt(0)};`,
        ),
      (span) => span,
    ),
  );
}

function stripHtmlOutsideMath(text: string, tableRow: boolean): string {
  return mapMathSpans(
    text,
    (plain) => stripHtmlChunk(plain, tableRow),
    (span) => span,
  );
}

/** The delimiter that closes display math opened on this line, if it stays open. */
function openDisplayMath(line: string): string | null {
  const trimmed = line.trim();
  const close = trimmed.startsWith("$$")
    ? "$$"
    : trimmed.startsWith("\\[")
      ? "\\]"
      : null;
  return close && !trimmed.slice(2).includes(close) ? close : null;
}

function stripHtmlLine(line: string, tableRow: boolean): string {
  let output = "";
  let index = 0;
  let inlineTicks = 0;
  let plainStart = 0;

  while (index < line.length) {
    if (line[index] !== "`") {
      index += 1;
      continue;
    }
    let count = 1;
    while (line[index + count] === "`") count += 1;
    if (inlineTicks === 0) {
      output += stripHtmlOutsideMath(line.slice(plainStart, index), tableRow);
      inlineTicks = count;
      plainStart = index;
    } else if (inlineTicks === count) {
      output += line.slice(plainStart, index + count);
      inlineTicks = 0;
      plainStart = index + count;
    }
    index += count;
  }

  output +=
    inlineTicks === 0
      ? stripHtmlOutsideMath(line.slice(plainStart), tableRow)
      : line.slice(plainStart);
  return output;
}

function isTableDelimiter(line: string): boolean {
  return (
    line.includes("|") &&
    /^\s*\|?\s*:?-+:?\s*(?:\|\s*:?-+:?\s*)*\|?\s*$/.test(line)
  );
}

/**
 * Remove raw HTML wrappers emitted by models while retaining their readable
 * contents, and keep superscripts and subscripts as math. Code examples and
 * math are kept byte-for-byte, and autolinks such as `<https://example.com>`
 * are not treated as tags.
 */
export function stripPresentationalHtml(markdown: string): string {
  let fence: { marker: string; count: number } | null = null;
  let displayMath: string | null = null;
  let inTable = false;
  return markdown
    .split(/(?<=\n)/)
    .map((line, index, lines) => {
      const marker = fenceMarker(line);
      if (marker) {
        if (
          fence &&
          marker.marker === fence.marker &&
          marker.count >= fence.count
        ) {
          fence = null;
        } else if (!fence) {
          fence = marker;
        }
        return line;
      }
      if (fence) return line;
      if (displayMath) {
        if (line.includes(displayMath)) displayMath = null;
        return line;
      }
      displayMath = openDisplayMath(line);
      if (displayMath) return line;
      inTable =
        line.includes("|") &&
        (inTable || isTableDelimiter(lines[index - 1] ?? ""));
      const tableRow =
        inTable ||
        /^\s*\|/.test(line) ||
        (line.includes("|") && isTableDelimiter(lines[index + 1] ?? ""));
      return stripHtmlLine(line, tableRow);
    })
    .join("");
}
