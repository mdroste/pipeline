import katex, { type KatexOptions } from "katex";

interface MarkdownNode {
  type: string;
  value?: string;
  children?: MarkdownNode[];
  data?: {
    hName?: string;
    hProperties?: Record<string, unknown>;
    hChildren?: HastNode[];
  };
}

interface HastNode {
  type: string;
  tagName?: string;
  value?: string;
  properties?: Record<string, unknown>;
  children?: HastNode[];
}

const GREEK_COMMANDS = [
  "alpha",
  "beta",
  "gamma",
  "delta",
  "epsilon",
  "varepsilon",
  "zeta",
  "eta",
  "theta",
  "vartheta",
  "iota",
  "kappa",
  "lambda",
  "mu",
  "nu",
  "xi",
  "pi",
  "varpi",
  "rho",
  "varrho",
  "sigma",
  "varsigma",
  "tau",
  "upsilon",
  "phi",
  "varphi",
  "chi",
  "psi",
  "omega",
  "Gamma",
  "Delta",
  "Theta",
  "Lambda",
  "Xi",
  "Pi",
  "Sigma",
  "Upsilon",
  "Phi",
  "Psi",
  "Omega",
];

const SYMBOL_COMMANDS = [
  ...GREEK_COMMANDS,
  "infty",
  "partial",
  "nabla",
  "ell",
  "hbar",
];

const MATH_COMMANDS = [
  ...SYMBOL_COMMANDS,
  "frac",
  "sqrt",
  "sum",
  "prod",
  "int",
  "oint",
  "lim",
  "mathbb",
  "mathbbm",
  "mathds",
  "mathbf",
  "mathrm",
  "mathit",
  "mathcal",
  "mathsf",
  "mathtt",
  "boldsymbol",
  "bm",
  "operatorname",
  "hat",
  "bar",
  "vec",
  "tilde",
  "dot",
  "ddot",
  "overline",
  "underline",
];

const COMMAND_PATTERN = MATH_COMMANDS.join("|");
const TEX_SIGNAL_RE = new RegExp(
  `\\\\(?:${COMMAND_PATTERN})(?![A-Za-z])|[A-Za-z0-9})]\\s*[_^]\\s*(?:\\{|[A-Za-z0-9\\\\])`,
);
const COMMAND_RE = new RegExp(`\\\\(?:${COMMAND_PATTERN})(?![A-Za-z])`, "g");
const INLINE_ATOM_RE = new RegExp(
  [
    String.raw`\\frac\s*\{[^{}\n]{1,160}\}\s*\{[^{}\n]{1,160}\}`,
    String.raw`\\(?:sqrt|mathbbm?|mathds|mathbf|mathrm|mathit|mathcal|mathsf|mathtt|boldsymbol|bm|operatorname|hat|bar|vec|tilde|dot|ddot|overline|underline)\s*\{[^{}\n]{1,160}\}(?:\s*[_^]\s*(?:\{[^{}\n]{1,80}\}|[A-Za-z0-9]))*`,
    String.raw`\\(?:${SYMBOL_COMMANDS.join("|")})(?![A-Za-z])(?:\s*[_^]\s*(?:\{[^{}\n]{1,80}\}|[A-Za-z0-9]))*`,
  ].join("|"),
  "g",
);

export const REPORT_KATEX_OPTIONS: KatexOptions = {
  strict: "ignore",
  trust: false,
  maxExpand: 1_000,
  maxSize: 20,
  macros: {
    "\\bm": "\\boldsymbol",
    "\\mathbbm": "\\mathbb",
    "\\mathds": "\\mathbb",
    "\\R": "\\mathbb{R}",
    "\\N": "\\mathbb{N}",
    "\\Z": "\\mathbb{Z}",
    "\\Q": "\\mathbb{Q}",
    "\\E": "\\mathbb{E}",
    "\\Var": "\\operatorname{Var}",
    "\\Cov": "\\operatorname{Cov}",
    "\\diag": "\\operatorname{diag}",
  },
};

/**
 * Apply only syntax-level repairs whose visual meaning is unambiguous.
 * Mathematical content, operators, scripts, and grouping are not inferred.
 */
export function repairLatexExpression(expression: string): string {
  return expression
    .replace(/\\begin\{split\}/g, "\\begin{aligned}")
    .replace(/\\end\{split\}/g, "\\end{aligned}")
    .replace(/\\(?:textnormal|textrm|textup)\s*\{/g, "\\text{")
    .replace(/\\label\s*\{[^{}]*\}/g, "")
    .replace(/\\eqref\s*\{([^{}]*)\}/g, "\\text{($1)}")
    .replace(/\\ref\s*\{([^{}]*)\}/g, "\\text{$1}")
    .trim();
}

export function canRenderLatex(
  expression: string,
  displayMode = false,
): boolean {
  try {
    katex.renderToString(repairLatexExpression(expression), {
      ...REPORT_KATEX_OPTIONS,
      displayMode,
      throwOnError: true,
    });
    return true;
  } catch {
    return false;
  }
}

function stripTextCommands(expression: string): string {
  return expression
    .replace(/\\(?:text|operatorname)\s*\{[^{}]*\}/g, "")
    .replace(COMMAND_RE, "")
    .replace(/\\./g, "")
    .replace(/[{}_^=+\-*/()[\],.;:<>|&0-9]/g, " ");
}

function containsProse(expression: string): boolean {
  return (stripTextCommands(expression).match(/[A-Za-z]{2,}/g) ?? []).some(
    (word) =>
      ![
        "arg",
        "cos",
        "det",
        "exp",
        "log",
        "max",
        "min",
        "mod",
        "sin",
        "tan",
      ].includes(word.toLowerCase()),
  );
}

function standaloneFormula(value: string): string | null {
  const candidate = value.trim();
  if (
    !candidate ||
    candidate.length > 4_000 ||
    candidate.includes("$") ||
    !TEX_SIGNAL_RE.test(candidate) ||
    containsProse(candidate)
  ) {
    return null;
  }
  const repaired = repairLatexExpression(candidate);
  return canRenderLatex(repaired, true) ? repaired : null;
}

function inlineFormula(value: string): string | null {
  const candidate = value.trim();
  if (
    !candidate ||
    candidate.length > 500 ||
    !TEX_SIGNAL_RE.test(candidate) ||
    containsProse(candidate)
  ) {
    return null;
  }
  const repaired = repairLatexExpression(candidate);
  return canRenderLatex(repaired) ? repaired : null;
}

function candidateRanges(value: string): Array<{
  start: number;
  end: number;
  formula: string;
}> {
  const candidates: Array<{ start: number; end: number; formula: string }> = [];
  const grouped = /(\([^()\n]{1,500}\)|\[[^[\]\n]{1,500}\])/g;
  for (const match of value.matchAll(grouped)) {
    const formula = inlineFormula(match[0]);
    if (formula && match.index !== undefined) {
      candidates.push({
        start: match.index,
        end: match.index + match[0].length,
        formula,
      });
    }
  }

  for (const match of value.matchAll(INLINE_ATOM_RE)) {
    if (match.index === undefined) continue;
    const start = match.index;
    const end = start + match[0].length;
    if (
      candidates.some(
        (candidate) => start < candidate.end && end > candidate.start,
      )
    ) {
      continue;
    }
    const formula = inlineFormula(match[0]);
    if (formula) candidates.push({ start, end, formula });
  }
  return candidates.sort((left, right) => left.start - right.start);
}

function splitTextNode(value: string): MarkdownNode[] {
  const ranges = candidateRanges(value);
  if (ranges.length === 0) return [{ type: "text", value }];

  const nodes: MarkdownNode[] = [];
  let cursor = 0;
  for (const range of ranges) {
    if (range.start > cursor) {
      nodes.push({ type: "text", value: value.slice(cursor, range.start) });
    }
    nodes.push(mathMarkdownNode(range.formula, false));
    cursor = range.end;
  }
  if (cursor < value.length) {
    nodes.push({ type: "text", value: value.slice(cursor) });
  }
  return nodes;
}

function mathMarkdownNode(value: string, display: boolean): MarkdownNode {
  const code: HastNode = {
    type: "element",
    tagName: "code",
    properties: {
      className: ["language-math", display ? "math-display" : "math-inline"],
    },
    children: [{ type: "text", value }],
  };
  return display
    ? {
        type: "math",
        value,
        data: { hName: "pre", hChildren: [code] },
      }
    : {
        type: "inlineMath",
        value,
        data: {
          hName: "code",
          hProperties: code.properties,
          hChildren: code.children,
        },
      };
}

function updateMathNode(node: MarkdownNode, value: string): void {
  node.value = value;
  const updateText = (candidate: HastNode): boolean => {
    if (candidate.type === "text") {
      candidate.value = value;
      return true;
    }
    return candidate.children?.some(updateText) ?? false;
  };
  node.data?.hChildren?.some(updateText);
}

function repairMarkdownTree(node: MarkdownNode): void {
  if (node.type === "math" || node.type === "inlineMath") {
    if (node.value !== undefined) {
      updateMathNode(node, repairLatexExpression(node.value));
    }
    return;
  }
  if (!node.children) return;

  for (let index = 0; index < node.children.length; index += 1) {
    const child = node.children[index];
    if (
      child.type === "paragraph" &&
      child.children?.length === 1 &&
      child.children[0].type === "text"
    ) {
      const formula = standaloneFormula(child.children[0].value ?? "");
      if (formula) {
        node.children[index] = mathMarkdownNode(formula, true);
        continue;
      }
    }
    repairMarkdownTree(child);
    if (child.children) {
      const repairedChildren: MarkdownNode[] = [];
      for (const grandchild of child.children) {
        if (grandchild.type === "text" && grandchild.value) {
          repairedChildren.push(...splitTextNode(grandchild.value));
        } else {
          repairedChildren.push(grandchild);
        }
      }
      child.children = repairedChildren;
    }
  }
}

/** remark plugin: repair parsed math and promote only high-confidence bare TeX. */
export function remarkRepairMath() {
  return (tree: MarkdownNode) => {
    repairMarkdownTree(tree);
  };
}

const READABLE_SYMBOLS: Record<string, string> = {
  alpha: "α",
  beta: "β",
  gamma: "γ",
  delta: "δ",
  epsilon: "ε",
  varepsilon: "ε",
  theta: "θ",
  lambda: "λ",
  mu: "μ",
  nu: "ν",
  xi: "ξ",
  pi: "π",
  rho: "ρ",
  sigma: "σ",
  tau: "τ",
  phi: "φ",
  varphi: "φ",
  chi: "χ",
  psi: "ψ",
  omega: "ω",
  infty: "∞",
  partial: "∂",
  nabla: "∇",
  sum: "Σ",
  prod: "Π",
  int: "∫",
  leq: "≤",
  geq: "≥",
  neq: "≠",
  times: "×",
  cdot: "·",
  approx: "≈",
  to: "→",
};

export function latexToReadableText(expression: string): string {
  let readable = expression.trim();
  for (let pass = 0; pass < 4; pass += 1) {
    const previous = readable;
    readable = readable
      .replace(/\\frac\s*\{([^{}]*)\}\s*\{([^{}]*)\}/g, "($1)/($2)")
      .replace(/\\sqrt\s*\{([^{}]*)\}/g, "√($1)")
      .replace(
        /\\(?:text|textnormal|textrm|textup|operatorname)\s*\{([^{}]*)\}/g,
        "$1",
      );
    if (readable === previous) break;
  }
  readable = readable
    .replace(/\\label\s*\{[^{}]*\}/g, "")
    .replace(/\\(?:left|right|displaystyle)\b/g, "")
    .replace(/\\(?:quad|qquad)\b|\\[,;!]/g, " ")
    .replace(/\\([A-Za-z]+)/g, (_match, command: string) => {
      return READABLE_SYMBOLS[command] ?? command;
    })
    .replace(/_\{([^{}]*)\}/g, "_($1)")
    .replace(/\^\{([^{}]*)\}/g, "^($1)")
    .replace(/\\\\/g, "; ")
    .replace(/[{}&]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return readable || "Unrendered formula";
}

function classNames(node: HastNode): string[] {
  const value = node.properties?.className;
  if (Array.isArray(value)) return value.map(String);
  if (typeof value === "string") return value.split(/\s+/);
  return [];
}

function hastText(node: HastNode): string {
  if (node.type === "text") return node.value ?? "";
  return node.children?.map(hastText).join("") ?? "";
}

function fallbackNode(
  source: string,
  error: unknown,
  display: boolean,
): HastNode {
  const readable = latexToReadableText(source);
  const detail = error instanceof Error ? error.message : String(error);
  return {
    type: "element",
    tagName: display ? "div" : "span",
    properties: {
      className: display
        ? ["math-fallback", "math-fallback-block"]
        : ["math-fallback"],
      title: `Original LaTeX: ${source}\n\n${detail}`,
      role: "note",
      tabIndex: 0,
      "aria-label": `Formula could not be rendered. ${readable}`,
      "data-original-latex": source,
    },
    children: [{ type: "text", value: readable }],
  };
}

function validateMathNodes(node: HastNode): void {
  if (!node.children) return;
  for (let index = 0; index < node.children.length; index += 1) {
    const child = node.children[index];
    const classes = classNames(child);
    const isMath =
      child.tagName === "code" &&
      (classes.includes("math-inline") || classes.includes("math-display"));
    if (isMath) {
      const source = hastText(child);
      const display = classes.includes("math-display");
      try {
        katex.renderToString(source, {
          ...REPORT_KATEX_OPTIONS,
          displayMode: display,
          throwOnError: true,
        });
      } catch (error) {
        const fallback = fallbackNode(source, error, display);
        if (display && node.tagName === "pre") {
          node.tagName = "div";
          node.properties = fallback.properties;
          node.children = fallback.children;
          return;
        }
        node.children[index] = fallback;
        continue;
      }
    }
    validateMathNodes(child);
  }
}

/**
 * rehype plugin placed before rehype-katex. Every parsed expression is checked
 * with the same KaTeX policy; invalid expressions become readable, inspectable
 * fallbacks rather than red source-code fragments.
 */
export function rehypeValidateMath() {
  return (tree: HastNode) => {
    validateMathNodes(tree);
  };
}
