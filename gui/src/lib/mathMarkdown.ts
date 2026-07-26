function fenceMarker(line: string): { marker: string; count: number } | null {
  const trimmed = line.trimStart();
  const marker = trimmed[0];
  if (marker !== "`" && marker !== "~") return null;
  let count = 0;
  while (trimmed[count] === marker) count += 1;
  return count >= 3 ? { marker, count } : null;
}

const DISPLAY_ENVIRONMENTS: Record<
  string,
  { open: string; close: string }
> = {
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

const RAW_HTML_TAG_RE =
  /<\s*\/?\s*[A-Za-z][A-Za-z0-9-]*(?:\s(?:[^>"']|"[^"]*"|'[^']*')*)?\s*\/?\s*>/g;
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

function stripHtmlChunk(chunk: string): string {
  return chunk.replace(RAW_HTML_TAG_RE, (tag) => {
    const name = tag
      .match(/^<\s*\/?\s*([A-Za-z][A-Za-z0-9-]*)/)?.[1]
      ?.toLowerCase();
    if (name === "br") return "\n";
    if (name && BLOCK_HTML_TAGS.has(name)) return "\n";
    return "";
  });
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
      output += stripHtmlChunk(line.slice(plainStart, index));
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
      ? stripHtmlChunk(line.slice(plainStart))
      : line.slice(plainStart);
  return output;
}

/**
 * Remove raw HTML wrappers emitted by models while retaining their readable
 * contents. Code examples are kept byte-for-byte, and autolinks such as
 * `<https://example.com>` are not treated as tags.
 */
export function stripPresentationalHtml(markdown: string): string {
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
      return fence ? line : stripHtmlLine(line);
    })
    .join("");
}
