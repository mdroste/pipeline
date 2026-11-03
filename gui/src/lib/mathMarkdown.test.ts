import katex from "katex";
import { describe, expect, it } from "vitest";
import {
  mathOnlyMarkdown,
  escapeTablePipes,
  normalizeMathDelimiters,
  removeHtmlTags,
  replaceMathSpans,
  stripPresentationalHtml,
} from "./mathMarkdown";
import { REPORT_KATEX_OPTIONS } from "./mathRepair";

const katexOutput = (markdown: string) =>
  katex.renderToString(markdown.replace(/^.*?\$|\$.*$/g, ""), {
    ...REPORT_KATEX_OPTIONS,
    throwOnError: false,
  });

describe("normalizeMathDelimiters", () => {
  it("normalizes inline and display math", () => {
    expect(normalizeMathDelimiters("a \\(x\\)\n\\[\ny\n\\]\n")).toBe(
      "a $x$\n$$\ny\n$$\n",
    );
  });

  it("preserves inline and fenced code", () => {
    const input = "`\\(x\\)`\n\n```tex\n\\[y\\]\n```\n";
    expect(normalizeMathDelimiters(input)).toBe(input);
  });

  it("normalizes standalone LaTeX display environments", () => {
    const input = [
      "\\begin{equation}",
      "y = x + 1",
      "\\end{equation}",
      "",
      "\\begin{align*}",
      "a &= b \\\\",
      "c &= d",
      "\\end{align*}",
      "",
    ].join("\n");
    expect(normalizeMathDelimiters(input)).toBe(
      [
        "$$",
        "y = x + 1",
        "$$",
        "",
        "$$",
        "\\begin{aligned}",
        "a &= b \\\\",
        "c &= d",
        "\\end{aligned}",
        "$$",
        "",
      ].join("\n"),
    );
  });

  it("removes raw presentation tags without changing code or autolinks", () => {
    const input = [
      "R<sup>2</sup> and x<sub>t</sub><br>Next",
      "",
      '<div><span id="finding">Finding</span></div>',
      "",
      "`<sup>literal</sup>` <https://example.com>",
      "",
      "```html",
      "<sup>literal block</sup>",
      "```",
      "",
    ].join("\n");

    expect(stripPresentationalHtml(input)).toBe(
      [
        "R\\(^{2}\\) and x\\(_{t}\\)",
        "Next",
        "",
        "",
        "Finding",
        "",
        "",
        "`<sup>literal</sup>` <https://example.com>",
        "",
        "```html",
        "<sup>literal block</sup>",
        "```",
        "",
      ].join("\n"),
    );
  });

  it("keeps prose and math inequalities that are not HTML tags", () => {
    // Regression: whitespace after `<` must disqualify a pseudo-tag, or
    // "T < N and R > 1" loses everything between the angle brackets.
    const prose = "We assume T < N and R > 1, so for all t < T and s > 0.";
    expect(stripPresentationalHtml(prose)).toBe(prose);

    const math = "Assume $a < b$ holds and $c > d$ fails when x < y and z > 0.";
    expect(stripPresentationalHtml(math)).toBe(math);
  });

  it.each([
    "if N<T and T>0 then we reject",
    "when x<y and z>w holds",
    // Each of these names a real element, so only the shape of the text helps.
    "if a<b and c>d then",
    "for p<q and s>0 and u<i and j>k",
    "Compare List<T> with Map<K, V> and <name>.",
    "We keep p<0.05 and n>30.",
  ])("keeps the unspaced inequality in %s", (text) => {
    expect(stripPresentationalHtml(text)).toBe(text);
  });

  it.each([
    "$i<j \\text{ and } k>l$",
    "Since $a<b>c$ holds, we stop.",
    "Display $$a<b>c$$ inline.",
    "\\(a<b>c\\) and \\[a<i>c\\]",
    "$$\na<b>c\n$$\n",
    "$$ a<b>c\nd<i>e $$\n",
    "\\[\na<b>c\n\\]\n",
  ])("keeps math byte for byte in %j", (text) => {
    expect(stripPresentationalHtml(text)).toBe(text);
  });

  it("still removes real tags around math and unpaired dollar signs", () => {
    expect(
      stripPresentationalHtml('<span class="x">Cost</span> of $a<b>c$<br/>'),
    ).toBe("Cost of $a<b>c$\n");
    expect(stripPresentationalHtml("It cost $5, <b>a lot</b>.")).toBe(
      "It cost $5, a lot.",
    );
    expect(stripPresentationalHtml("\\$5 <b>and</b> \\$6")).toBe(
      "\\$5 and \\$6",
    );
    expect(stripPresentationalHtml('<div class="a" hidden>x</div >')).toBe(
      "\nx\n",
    );
    expect(
      stripPresentationalHtml("<details open><DIV>y</DIV></details>"),
    ).toBe("\ny\n");
    expect(stripPresentationalHtml("<a href=x.md title='y'>z</a>")).toBe("z");
    expect(stripPresentationalHtml("x<y and <b>bold</b>")).toBe("x<y and bold");
  });
});

describe("superscripts and subscripts", () => {
  const shown = (text: string) =>
    normalizeMathDelimiters(stripPresentationalHtml(text));

  it("keeps the script on numbers and variables", () => {
    expect(shown("10<sup>6</sup> firms")).toBe("10$^{6}$ firms");
    expect(shown("p < 10<sup>-3</sup>")).toBe("p < 10$^{-3}$");
    expect(shown("x<sub>t</sub> and x<sub>1</sub>2")).toBe(
      "x$_{t}$ and x$_{1}$2",
    );
    expect(shown('10<SUP class="a">6</SUP>')).toBe("10$^{6}$");
  });

  it("does not pair a script with a currency sign on the same line", () => {
    expect(shown("It cost $5 for 10<sup>6</sup> firms.")).toBe(
      "It cost \\$5 for 10$^{6}$ firms.",
    );
    expect(shown("$5 and 10<sup>2</sup>")).toBe("\\$5 and 10$^{2}$");
  });

  it("escapes words so they cannot become TeX commands", () => {
    expect(shown("10<sup>th</sup>")).toBe("10$^{\\text{th}}$");
    expect(shown("x<sub>a_b$c</sub>")).toBe(
      "x$_{\\text{a\\_b\\textdollar{}c}}$",
    );
    expect(shown("x<sup><b>2</b></sup>")).toBe("x$^{2}$");
    expect(katexOutput(shown("x<sup>\\a{b}%&#^~†</sup>"))).not.toContain(
      "katex-error",
    );
  });

  it("marks a script that cannot be math, and drops an empty one", () => {
    expect(shown("10<sup>$x$</sup>")).toBe("10^$x$");
    expect(shown("x<sub>unclosed")).toBe("x_unclosed");
    expect(shown("| x<sup>a|b</sup> |")).toBe("| x^a|b |");
    expect(shown("x<sup> </sup>")).toBe("x");
  });

  it("leaves scripts in code and math alone", () => {
    const text = "`10<sup>6</sup>` and $a<sup>b$";
    expect(stripPresentationalHtml(text)).toBe(text);
  });
});

describe("HTML in table rows", () => {
  it("keeps a row on one line", () => {
    expect(stripPresentationalHtml("| Alpha | first<br>second |")).toBe(
      "| Alpha | first second |",
    );
    expect(stripPresentationalHtml("| a<br/>b | <p>c</p><p>d</p> |\n")).toBe(
      "| a b |  c  d  |\n",
    );
  });

  it("finds rows that do not start with a pipe", () => {
    expect(
      stripPresentationalHtml(
        ["h<br>1 | h2", "--- | ---", "a<br>b | c", "", "x<br>y"].join("\n"),
      ),
    ).toBe(["h 1 | h2", "--- | ---", "a b | c", "", "x", "y"].join("\n"));
  });

  it("still breaks lines outside a table", () => {
    expect(stripPresentationalHtml("a | b<br>c")).toBe("a | b\nc");
  });
});

describe("currency in math text", () => {
  it("keeps amounts as text and real math as math", () => {
    expect(normalizeMathDelimiters("between $500 and $1,000 per subject")).toBe(
      "between \\$500 and \\$1,000 per subject",
    );
    expect(normalizeMathDelimiters("$5-$8 and US$5 to US$8")).toBe(
      "\\$5-\\$8 and US\\$5 to US\\$8",
    );
    expect(normalizeMathDelimiters("$x$ and $y_i$ cost $5")).toBe(
      "$x$ and $y_i$ cost \\$5",
    );
    expect(normalizeMathDelimiters("a single $5\nthen $8 later")).toBe(
      "a single \\$5\nthen \\$8 later",
    );
    expect(normalizeMathDelimiters("costs $5 per unit and $y$ falls")).toBe(
      "costs \\$5 per unit and $y$ falls",
    );
    expect(normalizeMathDelimiters("Author $ ^{*} $ and $ \\beta $")).toBe(
      "Author $ ^{*} $ and $ \\beta $",
    );
  });

  it("leaves code, escapes, and display math alone", () => {
    for (const line of ["`$5 and $8`", "\\$5 and \\$8", "$$x + y$$"]) {
      expect(normalizeMathDelimiters(line)).toBe(line);
    }
    expect(normalizeMathDelimiters("```\n$5 and $8\n```\n")).toBe(
      "```\n$5 and $8\n```\n",
    );
  });

  it("does not let a currency pair hide tags from the cleaner", () => {
    expect(stripPresentationalHtml("From $5<br>to $8")).toBe("From $5\nto $8");
  });

  it("reads math out of plain text without touching amounts", () => {
    const shown = (text: string) =>
      replaceMathSpans(text, (math) => `[${math}]`);
    expect(shown("Costs of $5 and $10")).toBe("Costs of $5 and $10");
    expect(shown("Effect of $x$ on $$y$$")).toBe("Effect of [x] on [y]");
  });
});

describe("paragraph context for math", () => {
  it("keeps math that wraps within a paragraph", () => {
    expect(normalizeMathDelimiters("Let $a +\nb$ hold.\n")).toBe(
      "Let $a +\nb$ hold.\n",
    );
    expect(normalizeMathDelimiters("$a\n\nb$\n")).toBe("\\$a\n\nb\\$\n");
    expect(normalizeMathDelimiters("Footnote on $\\beta$1 here")).toBe(
      "Footnote on $\\beta$1 here",
    );
  });

  it("leaves dollars alone where Markdown would show an escape", () => {
    const unchanged = [
      "Code:\n\n    regress y $controls\n    summarize $x\n",
      "A span `$5\nand $8` wraps.\n",
      "See https://x.com/?a=$1 for it, and <https://x.com/a$b>.\n",
    ];
    for (const text of unchanged)
      expect(normalizeMathDelimiters(text)).toBe(text);
  });

  it("still treats indented list text as text", () => {
    expect(normalizeMathDelimiters("- item\n\n    costs $5 and $8\n")).toBe(
      "- item\n\n    costs \\$5 and \\$8\n",
    );
  });

  it("treats an unclosed backtick as text", () => {
    expect(normalizeMathDelimiters("it`s $5 and $8")).toBe(
      "it`s \\$5 and \\$8",
    );
  });
});

describe("scripts beside each other", () => {
  const clean = (text: string) =>
    normalizeMathDelimiters(stripPresentationalHtml(text));

  it("joins neighbouring scripts into one span", () => {
    expect(clean("x<sub>i</sub><sup>2</sup>")).toBe("x$_{i}{}^{2}$");
  });

  it("keeps a script after math from forming a $$ delimiter", () => {
    expect(clean("$x$<sup>2</sup>")).toBe("$x$\u2060$^{2}$");
  });

  it("keeps a dollar inside a script from closing the span", () => {
    expect(clean("a<sup>$5</sup>")).toBe("a$^{\\text{\\textdollar{}5}}$");
  });

  it("leaves Markdown inside a script working", () => {
    expect(clean("see<sup>[1](#ref1)</sup>")).toBe("see^[1](#ref1)");
  });
});

describe("removeHtmlTags", () => {
  it("drops elements and keeps inequalities", () => {
    expect(removeHtmlTags("Case N<T and T>0 holds")).toBe(
      "Case N<T and T>0 holds",
    );
    expect(removeHtmlTags('<span class="x">Costs</span> a<b and c>d <T>')).toBe(
      "Costs a<b and c>d <T>",
    );
  });
});

describe("mathOnlyMarkdown", () => {
  it("is null without math, including for two prices", () => {
    expect(mathOnlyMarkdown("Costs of $5 to $8")).toBeNull();
    expect(mathOnlyMarkdown("# A *plain* title")).toBeNull();
  });

  it("keeps math and turns all other punctuation into references", () => {
    expect(mathOnlyMarkdown("1. SEs_for *x*: $\\hat\\beta$")).toBe(
      "1&#46; SEs&#95;for &#42;x&#42;&#58; $\\hat\\beta$",
    );
    expect(mathOnlyMarkdown("\\(a_b\\) costs $5")).toBe("$a_b$ costs &#36;5");
  });
});

describe("a currency sign after a word or number", () => {
  it("stays text where no math follows it", () => {
    expect(normalizeMathDelimiters("GDP (US$) and exports (US$)")).toBe(
      "GDP (US\\$) and exports (US\\$)",
    );
    expect(normalizeMathDelimiters("in A$ and NZ$ terms")).toBe(
      "in A\\$ and NZ\\$ terms",
    );
    expect(normalizeMathDelimiters("USD/$ and EUR/$ rates")).toBe(
      "USD/\\$ and EUR/\\$ rates",
    );
    expect(normalizeMathDelimiters("the 5$ and 6$ tickets")).toBe(
      "the 5\\$ and 6\\$ tickets",
    );
  });

  it("still opens math when math follows it", () => {
    for (const text of ["Smith$^{1}$ wrote", "0.05$^{*}$", "Author$ ^{*} $"])
      expect(normalizeMathDelimiters(text)).toBe(text);
  });
});

describe("escapeTablePipes", () => {
  it("counts the backslashes before a pipe", () => {
    expect(escapeTablePipes("a\\\\|b")).toBe("a\\\\\\|b");
    expect(escapeTablePipes("$a\\\\|b$")).toBe("$a\\\\\\vert b$");
    expect(escapeTablePipes("a\\|b")).toBe("a\\|b");
  });
});
