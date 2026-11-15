/**
 * A bounded grammar for undelimited notation in older reports. It recognizes
 * complete runs of atoms, balanced groups and operators, with a Greek/script
 * signal. It never infers missing operators or subscripts. Unknown words and
 * malformed expressions stay text; callers protect Markdown code and links.
 */
export interface LegacyMathSpan {
  start: number;
  end: number;
  latex: string;
}

const GREEK = /^[Α-Ωα-ωϑϕϖϵ]$/u;
const LETTER = /^[A-Za-zΑ-Ωα-ωϑϕϖϵîÎŶŷ]$/u;
const WORD = /[\p{L}\p{N}\p{M}_]/u;
const SUPER: Record<string, string> = {
  "⁰": "0",
  "¹": "1",
  "²": "2",
  "³": "3",
  "⁴": "4",
  "⁵": "5",
  "⁶": "6",
  "⁷": "7",
  "⁸": "8",
  "⁹": "9",
  "⁺": "+",
  "⁻": "-",
  "⁽": "(",
  "⁾": ")",
  ⁿ: "n",
  ⁱ: "i",
  ᴸ: "L",
};
const OPERATORS = /^[=+−*/·×<>≤≥≈≠∈±-]$/u;
const TEX_OPERATOR: Record<string, string> = {
  "≤": "\\leq ",
  "≥": "\\geq ",
  "≈": "\\approx ",
  "≠": "\\neq ",
  "∈": "\\in ",
  "·": "\\cdot ",
  "×": "\\times ",
  "±": "\\pm ",
};
const MAX_SPAN = 500;
interface Part {
  end: number;
  latex: string;
  signal: boolean;
}

function parse(source: string, start: number): Part | null {
  const limit = Math.min(source.length, start + MAX_SPAN);
  const spaces = (at: number) => {
    while (at < limit && /[ \t]/.test(source[at])) at++;
    return at;
  };
  function atom(at: number, depth: number): Part | null {
    if (at >= limit || depth > 12) return null;
    let end = at;
    let latex = "";
    let signal = false;
    const c = source[at];
    if (c === "(" || c === "[" || c === "{") {
      const close = c === "(" ? ")" : c === "[" ? "]" : "}";
      const inner = expression(at + 1, depth + 1);
      if (!inner || source[spaces(inner.end)] !== close) return null;
      end = spaces(inner.end) + 1;
      latex = c + inner.latex + close;
      signal = inner.signal;
    } else if (/\d/.test(c)) {
      const number = source.slice(at, limit).match(/^\d+(?:\.\d+)?/)!;
      end += number[0].length;
      latex = number[0];
    } else if (LETTER.test(c)) {
      end++;
      latex = c;
      signal = GREEK.test(c);
      // Precomposed hats are accepted only as isolated mathematical atoms.
      const decomposed = c.normalize("NFD");
      if (decomposed.length === 2 && decomposed[1] === "\u0302") {
        latex = `\\hat{${decomposed[0]}}`;
        signal = true;
      }
      if (source[end] === "\u0302" || source[end] === "\u0303") {
        latex = `\\${source[end] === "\u0302" ? "hat" : "tilde"}{${latex}}`;
        signal = true;
        end++;
      }
      // Adjacent Latin letters are usually words, paths, or identifiers.
      // Products such as cλ and βq remain separate adjacent atoms.
      if (/[A-Za-z]/.test(c) && /[A-Za-z]/.test(source[end] ?? "")) return null;
      if (
        GREEK.test(c) &&
        GREEK.test(source[end] ?? "") &&
        GREEK.test(source[end + 1] ?? "")
      )
        return null;
    } else return null;

    const scripts = new Set<string>();
    while (end < limit) {
      const marker = source[end];
      if (marker === "_" || marker === "^") {
        if (scripts.has(marker)) return null;
        scripts.add(marker);
        const scriptStart = end + 1;
        let script: Part | null;
        if (source[scriptStart] === "{") {
          script = atom(scriptStart, depth + 1);
          if (!script) return null;
          latex += marker + script.latex;
        } else {
          const token = source
            .slice(scriptStart, limit)
            .match(/^(?:[A-Z]{1,4}|[a-zΑ-Ωα-ω]|\d+)(?![A-Za-zΑ-Ωα-ω0-9])/u);
          if (!token) return null;
          script = {
            end: scriptStart + token[0].length,
            latex: token[0],
            signal: true,
          };
          latex += `${marker}{${script.latex}}`;
        }
        end = script.end;
        signal = true;
      } else if (SUPER[marker]) {
        if (scripts.has("^")) return null;
        scripts.add("^");
        let script = "";
        while (end < limit && SUPER[source[end]])
          script += SUPER[source[end++]];
        latex += `^{${script}}`;
        signal = true;
      } else break;
    }
    return { end, latex, signal };
  }
  function expression(at: number, depth: number): Part | null {
    let current = atom(at, depth);
    if (!current) return null;
    while (current.end < limit) {
      const next = spaces(current.end);
      const c = source[next];
      if (OPERATORS.test(c ?? "")) {
        const right = atom(spaces(next + 1), depth);
        // An operator attached to a malformed expression invalidates the run.
        if (!right) return null;
        current = {
          end: right.end,
          latex:
            current.latex + " " + (TEX_OPERATOR[c] ?? c) + " " + right.latex,
          signal: current.signal || right.signal,
        };
      } else if (
        next === current.end &&
        (LETTER.test(c ?? "") || c === "(" || c === "[")
      ) {
        const right = atom(next, depth);
        if (!right) return null;
        current = {
          end: right.end,
          latex: current.latex + right.latex,
          signal: current.signal || right.signal,
        };
      } else if (next > current.end && current.signal) {
        // Whitespace can separate factors, but an ordinary word is never a
        // factor. Require notation on the next atom before joining it.
        const right = atom(next, depth);
        if (!right?.signal) break;
        current = {
          end: right.end,
          latex: current.latex + " " + right.latex,
          signal: true,
        };
      } else break;
    }
    return current;
  }
  return expression(start, 0);
}

/** Offsets always refer to the supplied source, never a normalized copy. */
export function legacyMathSpans(source: string): LegacyMathSpan[] {
  const spans: LegacyMathSpan[] = [];
  for (let start = 0; start < source.length;) {
    const previous = source[start - 1] ?? "";
    if (WORD.test(previous) || /[\\/$^]/.test(previous)) {
      start++;
      continue;
    }
    const part = parse(source, start);
    if (!part) {
      // Skip an unrecognized connected token as a whole. Its parentheses or
      // suffix must not become a second opportunity to guess at its meaning.
      const token = source
        .slice(start)
        .match(/^[\p{L}\p{N}\p{M}_^/()[\]{}+−=·²³ᴸ.-]+/u)?.[0];
      start += token?.length || 1;
      continue;
    }
    const next = source[part.end] ?? "";
    // Never salvage an inner fragment of an identifier, path, or broken formula.
    const invalidEnd =
      WORD.test(next) ||
      /[\\/^]/.test(next) ||
      /^\.[A-Za-z]/.test(source.slice(part.end));
    if (part.signal && !invalidEnd)
      spans.push({ start, end: part.end, latex: part.latex });
    start = Math.max(start + 1, part.end);
  }
  return spans;
}
