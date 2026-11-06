import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import CompactMarkdown, { InlineMathText } from "./CompactMarkdown";
import { renderSurvey } from "../../lib/surveyMarkdown";

it("shows a superscript as a superscript, even after a price", () => {
  const { container } = render(
    <CompactMarkdown markdown={"It cost $5 for 10<sup>6</sup> firms."} />,
  );
  const math = container.querySelectorAll(".katex");
  expect(math).toHaveLength(1);
  expect(math[0].querySelector(".msupsub")).not.toBeNull();
  expect(math[0].querySelector("annotation")).toHaveTextContent("^{6}");
  expect(container.querySelector(".katex-error")).toBeNull();
  expect(container).toHaveTextContent(/^It cost \$5 for 10.* firms\.$/);
});

it("keeps a cell with a line break in its own row and column", () => {
  render(
    <CompactMarkdown
      markdown={[
        "| Name | Note |",
        "| --- | --- |",
        "| Alpha | first<br>second |",
        "| Beta | third |",
      ].join("\n")}
    />,
  );
  const rows = within(screen.getByRole("table")).getAllByRole("row");
  expect(rows).toHaveLength(3);
  expect(
    within(rows[1])
      .getAllByRole("cell")
      .map((cell) => cell.textContent),
  ).toEqual(["Alpha", "first second"]);
});

it.each([
  ["x<sub>i</sub><sup>2</sup>", 1],
  ["$x$<sup>2</sup>", 2],
  ["a<sup>$5</sup>", 1],
  ["Let $a +\nb$ hold.", 1],
])("typesets %s without a math error", (markdown, spans) => {
  const { container } = render(<CompactMarkdown markdown={markdown} />);
  expect(container.querySelectorAll(".katex")).toHaveLength(spans);
  expect(container.querySelector(".katex-error")).toBeNull();
  // KaTeX keeps the source in its annotation, so read only the text around it.
  const prose = container.cloneNode(true) as HTMLElement;
  prose.querySelectorAll(".katex").forEach((math) => math.remove());
  expect(prose.textContent).not.toContain("$");
});

it("keeps a link inside a superscript", () => {
  render(<CompactMarkdown markdown={"see<sup>[1](https://x.org/a)</sup>"} />);
  expect(screen.getByRole("link", { name: "1" })).toBeInTheDocument();
});

it("typesets a conditional bar in a survey table, in its own column", () => {
  const markdown = renderSurvey({
    metadata: { title: "Paper", paper_type: "empirical" },
    notation: [
      {
        symbol: "$E[Y|X]$",
        definition: "Conditional mean",
        page_introduced: 4,
      },
    ],
  } as never);
  const { container } = render(<CompactMarkdown markdown={markdown} />);
  const row = within(screen.getByRole("table")).getAllByRole("row")[1];
  const cells = within(row).getAllByRole("cell");
  expect(cells).toHaveLength(3);
  expect(cells[1]).toHaveTextContent("Conditional mean");
  expect(cells[2]).toHaveTextContent("p. 4");
  expect(container.querySelector(".katex-error")).toBeNull();
  expect(cells[0].querySelector("annotation")).toHaveTextContent(
    "E[Y\\vert X]",
  );
});

describe("InlineMathText", () => {
  it("typesets bare notation in finding titles without interpreting their Markdown", () => {
    const { container } = render(
      <InlineMathText text="1. *Check* β₁ ≈ −1.3" />,
    );
    expect(container.querySelectorAll(".katex")).toHaveLength(1);
    expect(container.querySelector("annotation")).toHaveTextContent(
      "β_{1} ≈ −1.3",
    );
    expect(container.querySelector("ol, em, p")).toBeNull();
    expect(container).toHaveTextContent("1. *Check*");
  });

  it("keeps undelimited evidence quotes verbatim", () => {
    const { container } = render(<InlineMathText text="β₁ ≈ −1.3" verbatim />);
    expect(container.textContent).toBe("β₁ ≈ −1.3");
    expect(container.querySelector(".katex")).toBeNull();
  });
  it("typesets math and leaves the rest of a title as written", () => {
    const { container } = render(
      <InlineMathText text={"1. SEs_for *x* and $\\hat\\beta$ cost $5"} />,
    );
    expect(container.querySelectorAll(".katex")).toHaveLength(1);
    expect(container.querySelector(".katex-error")).toBeNull();
    expect(container.querySelector("ol, em, p")).toBeNull();
    const prose = container.cloneNode(true) as HTMLElement;
    prose.querySelectorAll(".katex").forEach((math) => math.remove());
    expect(prose).toHaveTextContent("1. SEs_for *x* and cost $5");
  });

  it("keeps a title without math as plain text", () => {
    const { container } = render(
      <InlineMathText text={"Costs of $5 to $8 <b>here</b>"} />,
    );
    expect(container.textContent).toBe("Costs of $5 to $8 here");
    expect(container.children).toHaveLength(0);
  });
});

it("typesets Unicode notation in finding explanations", () => {
  const { container } = render(
    <CompactMarkdown markdown="The coefficient β₁ ≈ −1.3 changes." />,
  );
  expect(container.querySelector("annotation")).toHaveTextContent(
    "β_{1} ≈ −1.3",
  );
  expect(container.querySelector(".math-fallback, .katex-error")).toBeNull();
});

it("never makes a link of a label, even with math in it", () => {
  const { container } = render(
    <InlineMathText text="See https://example.org/a or me@example.org for $x$" />,
  );
  expect(container.querySelector("a")).toBeNull();
  expect(container).toHaveTextContent("https://example.org/a");
  expect(container.querySelectorAll(".katex")).toHaveLength(1);
});

it("keeps currency in a label as written", () => {
  const { container } = render(
    <InlineMathText text="GDP (US$) and exports (US$)" />,
  );
  expect(container.querySelector(".katex")).toBeNull();
  expect(container).toHaveTextContent("GDP (US$) and exports (US$)");
});

it("keeps tag-like text in a verbatim quote and cleans a title", () => {
  const quote = render(<InlineMathText text="a<b>c holds" verbatim />);
  expect(quote.container).toHaveTextContent("a<b>c holds");
  const title = render(<InlineMathText text="Bold <b>word</b>" />);
  expect(title.container).toHaveTextContent("Bold word");
});
