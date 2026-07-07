import { describe, it, expect } from "vitest";
import { applyHighlights, clearHighlights } from "./useFindBar";

function container(html: string): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = html;
  return el;
}

describe("useFindBar highlight helpers", () => {
  it("wraps each case-insensitive match in a mark", () => {
    const el = container("<p>The Regression is a regression.</p>");
    const marks = applyHighlights(el, "regression");
    expect(marks).toHaveLength(2);
    expect(el.querySelectorAll("mark.search-hit")).toHaveLength(2);
    // Original text preserved (marks carry the matched casing).
    expect(el.textContent).toBe("The Regression is a regression.");
    expect(marks[0].textContent).toBe("Regression");
  });

  it("highlights across multiple elements", () => {
    const el = container("<p>alpha</p><p>alpha beta</p>");
    const marks = applyHighlights(el, "alpha");
    expect(marks).toHaveLength(2);
  });

  it("returns nothing for an empty query or no match", () => {
    const el = container("<p>hello</p>");
    expect(applyHighlights(el, "")).toHaveLength(0);
    expect(applyHighlights(el, "zzz")).toHaveLength(0);
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
    expect(applyHighlights(el, "word")).toHaveLength(1);
  });
});
