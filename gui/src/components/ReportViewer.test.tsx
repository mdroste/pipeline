import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ReportViewer, {
  plainHeadingText,
  stripInternalReportMarkers,
} from "./ReportViewer";
import { stripPresentationalHtml } from "../lib/mathMarkdown";

describe("ReportViewer", () => {
  it("renders markdown headings and body text", () => {
    render(<ReportViewer markdown={"# Report\n\nSome finding."} />);
    expect(screen.getByRole("heading", { name: "Report" })).toBeInTheDocument();
    expect(screen.getByText("Some finding.")).toBeInTheDocument();
  });

  it("shows a table of contents only when there are more than three headings", () => {
    const few = "# One\n\n## Two\n\ntext";
    const { rerender } = render(<ReportViewer markdown={few} />);
    expect(screen.queryByText("Contents")).not.toBeInTheDocument();

    const many = "# One\n\n## Two\n\n## Three\n\n## Four\n\ntext";
    rerender(<ReportViewer markdown={many} />);
    expect(screen.getByText("Contents")).toBeInTheDocument();
    // TOC links target slugified heading ids.
    const link = screen.getByRole("link", { name: "Four" });
    expect(link).toHaveAttribute("href", "#four");
  });

  it("presents formatted headings as clean plain-text TOC labels", () => {
    const markdown = [
      "# **Overview**",
      '## <span class="accent">Primary finding</span>',
      "## _Robustness checks_",
      "## [Appendix evidence](https://example.com)",
    ].join("\n\n");

    render(<ReportViewer markdown={markdown} />);
    const toc = screen.getByText("Contents").closest("nav");
    expect(toc).not.toBeNull();
    const tocQueries = within(toc!);

    expect(tocQueries.getByRole("link", { name: "Overview" })).toHaveAttribute(
      "href",
      "#overview",
    );
    expect(
      tocQueries.getByRole("link", { name: "Primary finding" }),
    ).toHaveAttribute("href", "#primary-finding");
    expect(
      tocQueries.getByRole("link", { name: "Robustness checks" }),
    ).toHaveAttribute("href", "#robustness-checks");
    expect(
      tocQueries.getByRole("link", { name: "Appendix evidence" }),
    ).toHaveAttribute("href", "#appendix-evidence");
    expect(toc).not.toHaveTextContent(/<span|\*\*|_/);
  });

  it("preserves literal formatting characters in heading labels", () => {
    expect(plainHeadingText("Use `**literal**` and \\*")).toBe(
      "Use **literal** and *",
    );
  });

  it("removes raw HTML wrappers from report content while preserving their text", () => {
    const markdown = [
      '# Finding',
      '<span id="finding-1" class="annotation">Visible finding</span>',
      "R<sup>2</sup> and x<sub>t</sub>",
    ].join("\n\n");
    const { container } = render(<ReportViewer markdown={markdown} />);

    expect(stripPresentationalHtml(markdown)).toContain("Visible finding");
    expect(screen.getByText("Visible finding")).toBeVisible();
    expect(screen.getByText("R2 and xt")).toBeVisible();
    expect(container.textContent).not.toContain("<span");
    expect(container.textContent).not.toContain("</span>");
    expect(container.textContent).not.toContain("<sup>");
    expect(container.textContent).not.toContain("<sub>");
  });

  it("lets the table of contents collapse independently", async () => {
    const user = userEvent.setup();
    render(<ReportViewer markdown={"# One\n\n## Two\n\n## Three\n\n## Four"} />);

    await user.click(screen.getByRole("button", { name: "Hide table of contents" }));
    expect(screen.queryByText("Contents")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show table of contents" }));
    expect(screen.getByText("Contents")).toBeVisible();
  });

  it("renders numbered issue headers as comment cards", () => {
    const { container } = render(
      <ReportViewer markdown={"**#1. Identification strategy is unclear**"} />,
    );
    expect(container.querySelector(".comment-num")?.textContent).toBe("1");
    expect(
      container.querySelector(".comment-title")?.textContent,
    ).toBe("Identification strategy is unclear");
  });

  it("renders math via KaTeX without showing the fallback notice", () => {
    const { container } = render(
      <ReportViewer markdown={"The estimate $\\beta = 0.5$ is implausible."} />,
    );
    expect(container.querySelector(".katex")).not.toBeNull();
    expect(
      screen.queryByText(/could not be rendered/),
    ).not.toBeInTheDocument();
  });

  it("normalizes legacy slash-delimited math before rendering", () => {
    const { container } = render(
      <ReportViewer markdown={"Inline \\(x+1\\).\n\n\\[\ny=2\n\\]"} />,
    );
    expect(container.querySelectorAll(".katex")).toHaveLength(2);
  });

  it("renders standalone LaTeX display environments through KaTeX", () => {
    const { container } = render(
      <ReportViewer
        markdown={"\\begin{equation}\ny=x+1\n\\end{equation}"}
      />,
    );
    expect(container.querySelector(".katex-display")).not.toBeNull();
    expect(container.textContent).not.toContain("\\begin{equation}");
  });

  it("promotes high-confidence bare TeX without rewriting prose", () => {
    const { container } = render(
      <ReportViewer
        markdown={[
          String.raw`x_t = \rho x_{t-1} + \varepsilon_t`,
          "",
          String.raw`The coefficient \alpha_t changes, but this prose stays prose.`,
        ].join("\n")}
      />,
    );

    expect(container.querySelectorAll(".katex")).toHaveLength(2);
    expect(screen.getByText(/but this prose stays prose/)).toBeVisible();
    const prose = container.cloneNode(true) as HTMLElement;
    prose.querySelectorAll(".katex").forEach((node) => node.remove());
    expect(prose.textContent).not.toContain("\\rho");
    expect(prose.textContent).not.toContain("\\alpha");
  });

  it("uses compatibility macros for common model-authored LaTeX", () => {
    const { container } = render(
      <ReportViewer markdown={String.raw`Vector: $\bm{x}\in\mathbbm{R}^n$.`} />,
    );

    expect(container.querySelector(".katex")).not.toBeNull();
    expect(container.querySelector(".math-fallback")).toBeNull();
  });

  it("shows an inspectable readable fallback for invalid LaTeX", () => {
    const { container } = render(
      <ReportViewer markdown={String.raw`Result: $\notARealCommand{\alpha_t}$.`} />,
    );
    const fallback = container.querySelector(".math-fallback");

    expect(fallback).not.toBeNull();
    expect(fallback).toHaveTextContent("notARealCommandα_t");
    expect(fallback).toHaveAttribute(
      "data-original-latex",
      String.raw`\notARealCommand{\alpha_t}`,
    );
    expect(fallback).toHaveAttribute("title", expect.stringContaining("Original LaTeX"));
  });

  it("does not normalize math-like delimiters inside code", () => {
    render(<ReportViewer markdown={"`\\(not math\\)`"} />);
    expect(screen.getByText("\\(not math\\)")).toBeInTheDocument();
  });

  it("never presents internal Pipeline run-detail boundary markers", () => {
    const markdown = [
      "# Report",
      "<!-- PIPELINE RUN DETAILS START -->",
      "**Model:** gpt-5.6-sol",
      "<!-- PIPELINE RUN DETAILS END -->",
      "Body.",
    ].join("\n");

    expect(stripInternalReportMarkers(markdown)).toBe(
      "# Report\n\n**Model:** gpt-5.6-sol\n\nBody.",
    );
    const { container } = render(<ReportViewer markdown={markdown} />);
    expect(container.textContent).not.toContain("PIPELINE RUN DETAILS");
    expect(screen.getByText("Model:")).toBeVisible();
    expect(screen.getByText("gpt-5.6-sol")).toBeVisible();
  });
});
