import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import ReportViewer from "../ReportViewer";
import type { PipelineReport } from "../../lib/reviewTypes";
import LocalSources from "./LocalSources";

const report = {
  orientation: {},
  step_outputs: [
    {
      step_id: "technical",
      step_label: "Technical",
      phase: "parallel",
      agent: "codex",
      raw_text: "Technical notes.",
    },
  ],
} as unknown as PipelineReport;

const reader = (container: HTMLElement) =>
  container.querySelector(".report-reader .overflow-y-auto") as HTMLElement;

it("shows host-rendered accepted edits while retaining the structured record", async () => {
  const editorial = {
    ...report,
    step_outputs: [
      {
        ...report.step_outputs[0],
        step_label: "Verify Meaning and Edit Interactions",
        raw_text:
          '{"decisions":[{"candidate_id":"edit:1","decision":"accept"}]}',
        structured_json: true,
        rendered_markdown:
          "## Recommended edits\n\n```text\nWages may rise by \\(x\\).\n```",
      },
    ],
  };
  render(
    <LocalSources
      ReportViewer={ReportViewer}
      extractedText="# Paper"
      report={editorial}
    />,
  );
  await userEvent.setup().click(
    screen.getByRole("button", {
      name: "Verify Meaning and Edit Interactions",
    }),
  );
  expect(
    screen.getByRole("heading", { name: "Recommended edits" }),
  ).toBeVisible();
  expect(screen.getByText("Wages may rise by \\(x\\).")).toBeVisible();
  expect(editorial.step_outputs[0].raw_text).toContain(
    '"candidate_id":"edit:1"',
  );
});

it("opens each source at the top", async () => {
  const { container } = render(
    <LocalSources
      ReportViewer={ReportViewer}
      extractedText="# Paper"
      report={report}
    />,
  );
  reader(container).scrollTop = 900;
  await userEvent
    .setup()
    .click(screen.getByRole("button", { name: "Technical" }));
  expect(screen.getByText("Technical notes.")).toBeVisible();
  expect(reader(container).scrollTop).toBe(0);
});

it("lets a long source list scroll", () => {
  render(
    <LocalSources
      ReportViewer={ReportViewer}
      extractedText="# Paper"
      report={report}
    />,
  );
  expect(screen.getByRole("navigation")).toHaveClass("overflow-y-auto");
});

it("says so when a cited page is not in the extracted text", () => {
  render(
    <LocalSources
      ReportViewer={ReportViewer}
      extractedText={"## Page 1\n\nText"}
      report={report}
      selectionRequest={{ key: 1, page: 412 }}
    />,
  );
  expect(screen.getByRole("status")).toHaveTextContent(
    "Page 412 is not in the extracted text.",
  );
});

it("adds no notice for a page it can show", () => {
  const previous = Object.getOwnPropertyDescriptor(
    HTMLElement.prototype,
    "scrollIntoView",
  );
  const scroll = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: scroll,
  });
  const errors = vi.spyOn(console, "error");
  try {
    render(
      <LocalSources
        ReportViewer={ReportViewer}
        extractedText={"## Page 12\n\nText"}
        report={report}
        selectionRequest={{ key: 1, page: 12 }}
      />,
    );
    const heading = screen.getByRole("heading", { name: "Page 12" });
    expect(scroll).toHaveBeenCalledWith({ block: "start" });
    expect(scroll.mock.instances.at(-1)).toBe(heading);
    expect(screen.queryByText(/not in the extracted text/)).toBeNull();
    expect(errors).not.toHaveBeenCalled();
  } finally {
    errors.mockRestore();
    if (previous)
      Object.defineProperty(HTMLElement.prototype, "scrollIntoView", previous);
    else delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
  }
});
