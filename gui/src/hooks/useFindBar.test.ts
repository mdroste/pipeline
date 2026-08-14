import { describe, it, expect } from "vitest";
import { applyHighlights, clearHighlights, MAX_FIND_MARKS } from "./useFindBar";

function container(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el;
}

describe("useFindBar highlight helpers", () => {
  it("wraps each case-insensitive match in a mark", () => {
    const el = container("<p>The Regression is a regression.</p>");
    const { marks, capped } = applyHighlights(el, "regression");
    expect(marks).toHaveLength(2);
    expect(capped).toBe(false);
    expect(el.querySelectorAll("mark.search-hit")).toHaveLength(2);
    // Original text preserved (marks carry the matched casing).
    expect(el.textContent).toBe("The Regression is a regression.");
    expect(marks[0].textContent).toBe("Regression");
  });

  it("highlights across multiple elements", () => {
    const el = container("<p>alpha</p><p>alpha beta</p>");
    const { marks } = applyHighlights(el, "alpha");
    expect(marks).toHaveLength(2);
  });

  it("returns nothing for an empty query or no match", () => {
    const el = container("<p>hello</p>");
    expect(applyHighlights(el, "").marks).toHaveLength(0);
    expect(applyHighlights(el, "zzz").marks).toHaveLength(0);
    expect(el.querySelectorAll("mark.search-hit")).toHaveLength(0);
  });

  it("clearHighlights restores the original text", () => {
    const el = container("<p>find the word find here</p>");
    applyHighlights(el, "find");
    expect(el.querySelectorAll("mark.search-hit")).toHaveLength(2);
    clearHighlights(el);
    expect(el.querySelectorAll("mark.search-hit")).toHaveLength(0);
    expect(el.textContent).toBe("find the word find here");
    // A second search works on the restored DOM.
    expect(applyHighlights(el, "word").marks).toHaveLength(1);
  });

  it("caps the number of applied marks and reports the cap", () => {
    const el = container(`<p>${"hit ".repeat(5)}</p><p>hit hit</p>`);
    const { marks, capped } = applyHighlights(el, "hit", 3);
    expect(marks).toHaveLength(3);
    expect(capped).toBe(true);
    expect(el.querySelectorAll("mark.search-hit")).toHaveLength(3);
    // Unmarked text is left intact so clearing restores everything.
    expect(el.textContent).toBe("hit hit hit hit hit hit hit");
    clearHighlights(el);
    expect(el.textContent).toBe("hit hit hit hit hit hit hit");
  });

  it("does not report a cap when matches exactly reach the limit", () => {
    const el = container("<p>one two one</p>");
    const { marks, capped } = applyHighlights(el, "one", 2);
    expect(marks).toHaveLength(2);
    expect(capped).toBe(false);
  });

  it("defaults the cap to MAX_FIND_MARKS", () => {
    expect(MAX_FIND_MARKS).toBe(1000);
  });

  it("skips matches inside KaTeX's hidden MathML source", () => {
    const el = container(
      '<p>gamma <span class="katex">' +
        '<span class="katex-mathml"><math><annotation>\\gamma</annotation></math></span>' +
        '<span class="katex-html">γ</span>' +
        "</span> gamma</p>",
    );
    const { marks } = applyHighlights(el, "gamma");
    expect(marks).toHaveLength(2);
    expect(el.querySelector(".katex-mathml mark")).toBeNull();
  });
});
