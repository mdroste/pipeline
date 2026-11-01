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

function normalizeLine(line: string): string {
  let output = "";
  let index = 0;
  let inlineTicks = 0;
  while (index < line.length) {
    if (line[index] === "`") {
      let count = 1;
      while (line[index + count] === "`") count += 1;
      output += line.slice(index, index + count);
      if (inlineTicks === 0) inlineTicks = count;
      else if (inlineTicks === count) inlineTicks = 0;
      index += count;
      continue;
    }
    if (
      inlineTicks === 0 &&
      line[index] === "\\" &&
      line[index - 1] !== "\\" &&
      index + 1 < line.length
    ) {
      const next = line[index + 1];
      if (next === "(" || next === ")") {
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
    output += line[index];
    index += 1;
  }
  return output;
}

/** Normalize legacy LaTeX math delimiters without changing code examples. */
export function normalizeMathDelimiters(markdown: string): string {
  let fence: { marker: string; count: number } | null = null;
  return markdown
    .split(/(?<=\n)/)
    .map((line) => {
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
      return normalizeDisplayEnvironment(line) ?? normalizeLine(line);
    })
    .join("");
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

function stripHtmlChunk(chunk: string): string {
  return chunk.replace(RAW_HTML_TAG_RE, (tag, tagName, attributes) => {
    const name = String(tagName).toLowerCase();
    const known =
      name === "br" || BLOCK_HTML_TAGS.has(name) || OTHER_HTML_TAGS.has(name);
    if (!known || !HTML_ATTRIBUTES_RE.test(attributes)) return tag;
    return name === "br" || BLOCK_HTML_TAGS.has(name) ? "\n" : "";
  });
}

// Math on one line, in every form normalizeMathDelimiters accepts, because the
// cleaner runs first. An escaped or unpaired `$` is text, as in remark-math.
const MATH_SPAN_RE =
  /(?<!\\)\$\$.+?\$\$|\\\[.+?\\\]|\\\(.+?\\\)|(?<![\\$])\$(?!\$)(?:\\.|[^$\\])+\$/g;

function stripHtmlOutsideMath(text: string): string {
  let output = "";
  let plainStart = 0;
  for (const span of text.matchAll(MATH_SPAN_RE)) {
    output += stripHtmlChunk(text.slice(plainStart, span.index)) + span[0];
    plainStart = span.index + span[0].length;
  }
  return output + stripHtmlChunk(text.slice(plainStart));
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

function stripHtmlLine(line: string): string {
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
      output += stripHtmlOutsideMath(line.slice(plainStart, index));
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
      ? stripHtmlOutsideMath(line.slice(plainStart))
      : line.slice(plainStart);
  return output;
}

/**
 * Remove raw HTML wrappers emitted by models while retaining their readable
 * contents. Code examples and math are kept byte-for-byte, and autolinks such
 * as `<https://example.com>` are not treated as tags.
 */
export function stripPresentationalHtml(markdown: string): string {
  let fence: { marker: string; count: number } | null = null;
  let displayMath: string | null = null;
  return markdown
    .split(/(?<=\n)/)
    .map((line) => {
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
      return displayMath ? line : stripHtmlLine(line);
    })
    .join("");
}
