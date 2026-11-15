// Recover unambiguous notation from older reports without rewriting saved text.
// Candidates need Greek letters, Unicode scripts, or explicit TeX scripts;
// ordinary words, currency, and ASCII arithmetic alone are not enough.
const LETTER = "A-Za-zα-ωΑ-Ωϵϑϖϱςϕ";
const SCRIPTS = "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁿⁱ₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ";
const SIGNAL = new RegExp(`[α-ωΑ-Ωϵϑϖϱςϕ${SCRIPTS}]|[_^]`);
const SCRIPT = String.raw`(?:[_^](?:\{[^{}\n]{1,80}\}|[${LETTER}0-9])|[${SCRIPTS}])+`;
const ATOM = new RegExp(
  String.raw`(?:[${LETTER}]+|\d+(?:\.\d+)?)(?:${SCRIPT})?`,
  "y",
);
const OPERATOR = /\s*(?:[=+−\-*/<>≤≥≠≈×·∈→])\s*/y;
const CLOSE: Record<string, string> = { "(": ")", "[": "]" };

export interface BareMathRange {
  start: number;
  end: number;
  formula: string;
}

function atomEnd(text: string, start: number, depth: number): number {
  if (depth > 8) return start;
  if (/[+−-]/.test(text[start] ?? "")) {
    const end = atomEnd(text, start + 1, depth + 1);
    return end > start + 1 ? end : start;
  }
  const close = CLOSE[text[start]];
  if (close) {
    const end = expressionEnd(text, start + 1, depth + 1);
    return end > start + 1 && text[end] === close ? end + 1 : start;
  }
  ATOM.lastIndex = start;
  const match = ATOM.exec(text);
  if (!match) return start;
  // Script contents can be words (e.g. x_{initial}), but prose cannot become
  // a product of letters. Mixed-case products such as pK are conventional.
  const base = match[0]
    .replace(/[_^].*$/, "")
    .replace(new RegExp(`[${SCRIPTS}]`, "g"), "");
  if (
    (base.match(/[A-Za-z]{2,}/g) ?? []).some(
      (word) => !/^[a-z]?[A-Z]+$/.test(word),
    )
  )
    return start;
  return start + match[0].length;
}

function expressionEnd(text: string, start: number, depth = 0): number {
  let cursor = start;
  if (/[+−-]/.test(text[cursor] ?? "")) cursor += 1;
  let end = atomEnd(text, cursor, depth);
  if (end === cursor) return start;
  while (end - start < 500) {
    OPERATOR.lastIndex = end;
    const operator = OPERATOR.exec(text);
    const next = operator ? end + operator[0].length : end;
    const following = atomEnd(text, next, depth);
    if (following === next) break;
    end = following;
  }
  return end;
}

/** Only scripts are translated; operators, grouping and variable names stay. */
export function normalizeUnicodeScripts(expression: string): string {
  const convert = (chars: string, replacements: string, marker: string) => {
    const map = Object.fromEntries(
      [...chars].map((char, i) => [char, replacements[i]]),
    );
    expression = expression.replace(
      new RegExp(`[${chars}]+`, "g"),
      (run) => `${marker}{${[...run].map((char) => map[char]).join("")}}`,
    );
  };
  convert("⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁿⁱ", "0123456789+-=()ni", "^");
  convert(
    "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓ",
    "0123456789+-=()aehijklmnoprstuvx",
    "_",
  );
  return expression;
}

export function bareMathRanges(text: string): BareMathRange[] {
  const ranges: BareMathRange[] = [];
  let start = 0;
  while (start < text.length) {
    // Consume words as a unit, never promoting the tail of a rejected word.
    const end = expressionEnd(text, start);
    if (end > start) {
      const candidate = text.slice(start, end);
      if (
        candidate.length <= 500 &&
        SIGNAL.test(candidate) &&
        !/[\p{L}\p{N}_\\/]/u.test(text[start - 1] ?? "") &&
        !/[\p{L}\p{N}\p{M}_^]/u.test(text[end] ?? "") &&
        !/^\.[A-Za-z]/.test(text.slice(end))
      ) {
        ranges.push({
          start,
          end,
          formula: normalizeUnicodeScripts(candidate),
        });
      }
      start = end;
    } else {
      const word = text.slice(start).match(/^[\p{L}\p{N}_]+/u);
      start += word?.[0].length ?? 1;
    }
  }
  return ranges;
}
