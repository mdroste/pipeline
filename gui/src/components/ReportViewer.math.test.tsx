import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import ReportViewer from "./ReportViewer";
import CompactMarkdown, {
  InlineMathText,
} from "./report-viewer/CompactMarkdown";

const formulas = [
  ["κ_w = cλ_w(1−β)", "κ_{w} = cλ_{w}(1 − β)"],
  ["sτ_t/(1−B_w)", "sτ_{t} / (1 − B_{w})"],
  ["β_IV = βq/[s(1−β)+βq]", "β_{IV} = βq / [s(1 − β) + βq]"],
  ["σ̂_K = 0.67", String.raw`\hat{σ}_{K} = 0.67`],
];
function annotations(container: HTMLElement) {
  return [...container.querySelectorAll("annotation")].map(
    (n) => n.textContent,
  );
}
describe("report math presentation", () => {
  it.each(formulas)(
    "typesets %s consistently in bodies and titles",
    (source, expected) => {
      const { container } = render(
        <>
          <ReportViewer markdown={`Result: ${source}.`} />
          <CompactMarkdown markdown={`Result: ${source}.`} />
          <InlineMathText text={`Result: ${source}.`} />
        </>,
      );
      expect(annotations(container)).toEqual([expected, expected, expected]);
      expect(
        container.querySelector(".math-fallback, .katex-error"),
      ).toBeNull();
      expect(container.querySelectorAll(".msupsub").length).toBeGreaterThan(0);
    },
  );
  it("leaves verbatim evidence intact while typesetting authored notation", () => {
    const { container } = render(<InlineMathText text="σ̂_K = 0.67" verbatim />);
    expect(container).toHaveTextContent("σ̂_K = 0.67");
    expect(container.querySelector(".katex")).toBeNull();
  });
  it("preserves currency and prose without silently treating them as equations", () => {
    const { container } = render(
      <ReportViewer
        markdown={"It costs $1M (1996$).\n\nThe slope κ_w = 0.2 is estimated."}
      />,
    );
    expect(container).toHaveTextContent("It costs $1M (1996$).");
    expect(annotations(container)).toEqual(["κ_{w} = 0.2"]);
  });
  it("decodes structured finding text without losing TeX escapes", () => {
    const raw = JSON.stringify({
      title: "Estimated slope",
      body: String.raw`The slope $\kappa_w = c\lambda_w(1-\beta)$ costs \$5.`,
    });
    const finding = JSON.parse(raw);
    const { container } = render(
      <ReportViewer markdown={`**#1. ${finding.title}**\n\n${finding.body}`} />,
    );
    expect(annotations(container)).toEqual([
      String.raw`\kappa_w = c\lambda_w(1-\beta)`,
    ]);
    expect(container).toHaveTextContent("costs $5.");
  });
  it("keeps legacy equations working across report parts and navigation", () => {
    const markdown =
      "# First\n\n" +
      "A short paragraph.\n\n".repeat(4200) +
      "\n## Last\n\nThe slope κ_w = cλ_w(1−β) holds.";
    const { container } = render(<ReportViewer markdown={markdown} />);
    const select = screen.getByLabelText("Report part") as HTMLSelectElement;
    fireEvent.change(select, {
      target: { value: select.options[select.options.length - 1].value },
    });
    expect(annotations(container)).toContain("κ_{w} = cλ_{w}(1 − β)");
    expect(screen.getByRole("heading", { name: "Last" }).id).not.toBe("");
  });
});

it("scrolls to a math title using the navigation identity, not duplicated MathML text", () => {
  const scroll = vi.fn();
  const previous = HTMLElement.prototype.scrollIntoView;
  HTMLElement.prototype.scrollIntoView = scroll;
  try {
    render(
      <ReportViewer
        markdown={"## Estimate σ_K\n\nBody."}
        scrollRequest={{ key: 1, heading: "Estimate σ_(K)" }}
      />,
    );
    expect(scroll).toHaveBeenCalledWith({ block: "start" });
  } finally {
    HTMLElement.prototype.scrollIntoView = previous;
  }
});
