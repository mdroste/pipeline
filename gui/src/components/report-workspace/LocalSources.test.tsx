import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import ReportViewer from "../ReportViewer";
import type { PipelineReport } from "../../lib/types";
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
  render(
    <LocalSources
      ReportViewer={ReportViewer}
      extractedText={"## Page 12\n\nText"}
      report={report}
      selectionRequest={{ key: 1, page: 12 }}
    />,
  );
  expect(screen.queryByText(/not in the extracted text/)).toBeNull();
});
