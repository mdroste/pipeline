// Command arguments are typed as a JSON-style list of quoted strings. Plain
// JSON is a trap for Windows paths: "C:\Program Files\…" is a syntax error,
// and "D:\tools\new.do" parses "successfully" into a tab and a newline.
//
// The rule here: text that is ordinary JSON whose only escapes are \\, \",
// and \/ (what JSON.stringify writes for a path) is read as JSON. Otherwise
// every backslash is kept as typed, and \" stays an escaped quote unless it
// closes the argument, as in "C:\out\".

export const ARGV_TEXT_HELP =
  'Enter command arguments as a list of quoted text values, such as ["python", "main.py"]. Windows paths can be typed as they are.';

function stringList(value: unknown): string[] | null {
  return Array.isArray(value) && value.every((item) => typeof item === "string")
    ? value
    : null;
}

/** Whether every escape inside a string literal is one a path needs. */
function onlyPathEscapes(text: string): boolean {
  let inString = false;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (!inString) {
      if (ch === '"') inString = true;
      continue;
    }
    if (ch === '"') inString = false;
    else if (ch === "\\") {
      const next = text[i + 1];
      if (next !== "\\" && next !== '"' && next !== "/") return false;
      i++;
    }
  }
  return true;
}

function strictList(text: string): string[] | null {
  if (!onlyPathEscapes(text)) return null;
  try {
    return stringList(JSON.parse(text));
  } catch {
    return null;
  }
}

const space = (ch: string | undefined) =>
  ch === " " || ch === "\t" || ch === "\n" || ch === "\r";

/** A list of quoted strings whose backslashes are literal. */
function literalList(text: string): string[] | null {
  let i = 0;
  const skip = () => {
    while (space(text[i])) i++;
  };
  skip();
  if (text[i] !== "[") return null;
  i++;
  skip();
  const values: string[] = [];
  if (text[i] === "]") {
    i++;
    skip();
    return i === text.length ? values : null;
  }
  for (;;) {
    skip();
    if (text[i] !== '"') return null;
    i++;
    let value = "";
    for (;;) {
      if (i >= text.length) return null;
      const ch = text[i];
      if (ch === '"') {
        i++;
        break;
      }
      if (ch === "\n" || ch === "\r") return null;
      if (ch === "\\" && text[i + 1] === '"') {
        let next = i + 2;
        while (space(text[next])) next++;
        if (text[next] === "," || text[next] === "]") {
          // A trailing backslash before the closing quote: "C:\out\".
          value += "\\";
          i += 2;
          break;
        }
        value += '"';
        i += 2;
        continue;
      }
      value += ch;
      i++;
    }
    values.push(value);
    skip();
    if (text[i] === ",") {
      i++;
      continue;
    }
    if (text[i] === "]") {
      i++;
      skip();
      return i === text.length ? values : null;
    }
    return null;
  }
}

/** The arguments in `text`, or null when it is not a list of quoted values. */
export function readArgvText(text: string): string[] | null {
  return strictList(text) ?? literalList(text);
}

/** The arguments in `text`; throws a message the researcher can act on. */
export function parseArgvText(text: string): string[] {
  const argv = readArgvText(text);
  if (!argv) throw new Error(ARGV_TEXT_HELP);
  return argv;
}
