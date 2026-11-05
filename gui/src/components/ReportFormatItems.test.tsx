import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ExportControls from "./ExportControls";
import { reviewedFileKind } from "./ReportFormatItems";

const invoke = vi.hoisted(() => vi.fn());
const notify = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./DialogService", () => ({ confirmDialog: vi.fn(), notify }));

async function openMenu() {
  await userEvent.setup().click(screen.getByRole("button", { name: /Export/ }));
}

function item(name: string) {
  return screen.getByRole("menuitem", { name: new RegExp(`^${name}`) });
}

describe("report format exports", () => {
  beforeEach(() => {
    invoke.mockReset();
    notify.mockReset();
  });

  it("recognizes the reviewed file type from the saved input path", () => {
    expect(reviewedFileKind("/papers/Draft.DOCX")).toBe("docx");
    expect(reviewedFileKind("C:\\papers\\paper.Pdf")).toBe("pdf");
    expect(reviewedFileKind("/papers/main.tex")).toBeNull();
    expect(reviewedFileKind("/papers/project")).toBeNull();
    expect(reviewedFileKind(undefined)).toBeNull();
  });

  it("offers plain-language formats and enables only the matching original", async () => {
    render(
      <ExportControls
        runId="run-1"
        markdown="# report"
        inputPath="/papers/draft.docx"
      />,
    );
    await openMenu();
    expect(item("Export as HTML")).toBeEnabled();
    expect(item("Export as Word")).toBeEnabled();
    expect(item("Word with comments on the original")).toBeEnabled();
    expect(item("PDF with highlights")).toBeDisabled();
    expect(item("PDF with highlights")).toHaveTextContent(
      "Available when the reviewed file is a PDF.",
    );
    expect(
      screen.getByText(/quotations from the manuscript and model-written/),
    ).toBeVisible();
  });

  it("disables both originals for LaTeX or folder inputs", async () => {
    render(
      <ExportControls
        runId="run-1"
        markdown="# report"
        inputPath="/papers/main.tex"
      />,
    );
    await openMenu();
    expect(item("Word with comments on the original")).toBeDisabled();
    expect(item("PDF with highlights")).toBeDisabled();
    expect(item("Export as Word")).toBeEnabled();
  });

  it("is offered only for saved runs", async () => {
    render(<ExportControls markdown="# report" inputPath="/p/a.pdf" />);
    await openMenu();
    expect(
      screen.queryByRole("menuitem", { name: /Export as HTML/ }),
    ).toBeNull();
  });

  it("saves HTML through the backend dialog without passing a path", async () => {
    invoke.mockResolvedValueOnce({
      path: "/Users/me/Desktop/draft review.html",
      findings: 4,
      anchored: 0,
      unanchored: 0,
    });
    render(<ExportControls runId="run-1" markdown="# report" />);
    await openMenu();
    await userEvent.setup().click(item("Export as HTML"));
    expect(invoke).toHaveBeenCalledWith("save_report_html", {
      runId: "run-1",
    });
    expect(notify).toHaveBeenCalledWith("Saved draft review.html.", "success");
  });

  it("reports how many findings were placed on the original", async () => {
    invoke.mockResolvedValueOnce({
      path: "/tmp/paper with review highlights.pdf",
      findings: 5,
      anchored: 3,
      unanchored: 2,
    });
    render(
      <ExportControls
        runId="run-1"
        markdown="# report"
        inputPath="/tmp/paper.pdf"
      />,
    );
    await openMenu();
    await userEvent.setup().click(item("PDF with highlights"));
    expect(invoke).toHaveBeenCalledWith("save_report_annotated_pdf", {
      runId: "run-1",
    });
    expect(notify).toHaveBeenCalledWith(
      "Saved paper with review highlights.pdf. 3 of 5 findings highlighted; 2 added as page notes.",
      "success",
    );
  });

  it("stays quiet when the save dialog is cancelled and explains failures", async () => {
    invoke.mockResolvedValueOnce(null);
    render(
      <ExportControls
        runId="run-1"
        markdown="# report"
        inputPath="/tmp/draft.docx"
      />,
    );
    await openMenu();
    await userEvent.setup().click(item("Word with comments on the original"));
    expect(invoke).toHaveBeenCalledWith("save_report_docx_comments", {
      runId: "run-1",
    });
    expect(notify).not.toHaveBeenCalled();

    invoke.mockRejectedValueOnce(
      new Error("The Word (.docx) file has changed since this Review ran"),
    );
    await openMenu();
    await userEvent.setup().click(item("Export as Word"));
    expect(notify).toHaveBeenCalledWith(
      "Export failed: The Word (.docx) file has changed since this Review ran",
    );
  });
});
