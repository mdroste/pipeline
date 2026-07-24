import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import ReportViewer from "./ReportViewer";

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

  it("does not normalize math-like delimiters inside code", () => {
    render(<ReportViewer markdown={"`\\(not math\\)`"} />);
    expect(screen.getByText("\\(not math\\)")).toBeInTheDocument();
  });
});
