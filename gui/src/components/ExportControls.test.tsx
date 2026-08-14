import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ExportControls from "./ExportControls";
import type { PipelineReport } from "../lib/types";

const openDialog = vi.hoisted(() => vi.fn());
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn(), open: openDialog }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const fakeReport: PipelineReport = {
  orientation: {
    metadata: {
      title: "t",
      authors: [],
      date: null,
      paper_type: "theory",
      page_count: null,
      has_appendix: false,
      has_online_appendix: false,
    },
    sections: [],
    formal_results: [],
    tables_figures: [],
    notation: [],
    stated_contribution: "",
    key_references: [],
    extraction_quality_notes: [],
  },
  step_outputs: [],
  report_date: "2026-04-16",
  paper_hash: "abc",
};

describe("ExportControls", () => {
  beforeEach(() => {
    openDialog.mockReset();
    invoke.mockReset();
  });

  it("hides the core-files export when report or extracted text is missing", () => {
    render(<ExportControls markdown="# hi" />);
    expect(screen.getByRole("button", { name: "Save MD" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save PDF" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Export core files" })).not.toBeInTheDocument();
  });

  it("shows the core-files export even when extraction produced empty text", () => {
    render(<ExportControls markdown="# hi" report={fakeReport} extractedText="" />);
    expect(screen.getByRole("button", { name: "Export core files" })).toBeInTheDocument();
  });

  it("shows the core-files export when report and extracted text are provided", () => {
    render(<ExportControls markdown="# hi" report={fakeReport} extractedText="text" />);
    expect(screen.getByRole("button", { name: "Export core files" })).toBeInTheDocument();
  });

  it("offers a complete export instead of the core package for a saved run", () => {
    render(
      <ExportControls
        runId="saved-run"
        markdown="# hi"
        report={fakeReport}
        extractedText="text"
      />,
    );
    expect(screen.getByRole("button", { name: "Export complete report" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Export core files" })).not.toBeInTheDocument();
  });

  it("invokes save_report_md with the markdown and a suggested name (no webview path)", async () => {
    invoke.mockResolvedValueOnce("/tmp/out.md");

    render(<ExportControls markdown="# report" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save MD" }));

    expect(invoke).toHaveBeenCalledTimes(1);
    const [command, args] = invoke.mock.calls[0];
    expect(command).toBe("save_report_md");
    expect(args.markdown).toBe("# report");
    expect(args.suggestedName).toMatch(/^PIPELINE_REPORT_.*\.md$/);
    // The webview must not pass a filesystem path — the backend owns the dialog.
    expect(args).not.toHaveProperty("path");
  });

  it("treats a cancelled save (backend returns null) without error", async () => {
    invoke.mockResolvedValueOnce(null);
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});

    render(<ExportControls markdown="# report" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save MD" }));

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(alertSpy).not.toHaveBeenCalled();
    alertSpy.mockRestore();
  });

  it("invokes print_report_html when 'Save PDF' is clicked", async () => {
    invoke.mockResolvedValueOnce(undefined);

    render(<ExportControls markdown="# report" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save PDF" }));

    expect(invoke).toHaveBeenCalledWith("print_report_html", {
      markdown: "# report",
      provenanceMarkdown: null,
    });
  });

  it("uses the clean report body and provenance masthead for PDF", async () => {
    invoke.mockResolvedValueOnce(undefined);

    render(
      <ExportControls
        markdown="# raw report"
        pdfMarkdown="## First issue"
        provenanceMarkdown={"# Referee report\n\n## Run provenance"}
      />,
    );
    await userEvent.setup().click(screen.getByRole("button", { name: "Save PDF" }));

    expect(invoke).toHaveBeenCalledWith("print_report_html", {
      markdown: "## First issue",
      provenanceMarkdown: "# Referee report\n\n## Run provenance",
    });
  });

  it("alerts with the backend error message on save failure", async () => {
    invoke.mockRejectedValueOnce(new Error("disk full"));
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});

    render(<ExportControls markdown="# report" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save MD" }));

    expect(alertSpy).toHaveBeenCalledWith("Failed to save: disk full");
    alertSpy.mockRestore();
  });

  it("confirms overwrite scope before exporting core files", async () => {
    openDialog.mockResolvedValueOnce("/tmp/artifacts");
    invoke.mockResolvedValueOnce(undefined);
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});

    render(<ExportControls markdown="# r" report={fakeReport} extractedText="text" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Export core files" }));

    expect(openDialog).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
      title: "Choose folder for core report files",
    });
    expect(confirmSpy.mock.calls[0][0]).toContain(
      "Existing files and earlier exports will not be replaced.",
    );
    expect(confirmSpy.mock.calls[0][0]).toContain("document.md");
    expect(confirmSpy.mock.calls[0][0]).not.toContain("extracted_text.md");
    expect(invoke).toHaveBeenCalledWith("save_all_artifacts", {
      dir: "/tmp/artifacts",
      markdown: "# r",
      extractedText: "text",
      report: fakeReport,
    });
    expect(alertSpy).toHaveBeenCalledWith(
      "Core report files exported to a new pipeline-core-export folder. Existing files were not replaced.",
    );
    confirmSpy.mockRestore();
    alertSpy.mockRestore();
  });

  it("does not export core files when overwrite confirmation is declined", async () => {
    openDialog.mockResolvedValueOnce("/tmp/artifacts");
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);

    render(<ExportControls markdown="# r" report={fakeReport} extractedText="text" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Export core files" }));

    expect(invoke).not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  it("surfaces core-export folder-picker failures", async () => {
    openDialog.mockRejectedValueOnce(new Error("folder dialog unavailable"));
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});

    render(<ExportControls markdown="# r" report={fakeReport} extractedText="text" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Export core files" }));

    expect(alertSpy).toHaveBeenCalledWith(
      "Failed to export core report files: folder dialog unavailable",
    );
    expect(invoke).not.toHaveBeenCalled();
    alertSpy.mockRestore();
  });

  it("exports a complete saved run into the chosen parent directory", async () => {
    openDialog.mockResolvedValueOnce("/tmp/exports");
    invoke.mockResolvedValueOnce({
      exportedPath: "/tmp/exports/pipeline-run-saved-run",
      fileCount: 24,
      bytes: 1_500_000,
    });
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});

    render(<ExportControls runId="saved-run" markdown="# r" />);
    await userEvent.setup().click(
      screen.getByRole("button", { name: "Export complete report" }),
    );

    expect(openDialog).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
      title: "Choose parent folder for complete report export",
    });
    expect(invoke).toHaveBeenCalledWith("export_run_artifacts", {
      runId: "saved-run",
      destination: "/tmp/exports",
    });
    expect(alertSpy).toHaveBeenCalledWith(
      "Complete report exported to /tmp/exports/pipeline-run-saved-run (24 files, 1.5 MB).",
    );
    alertSpy.mockRestore();
  });

  it("does not invoke complete-run export when folder selection is cancelled", async () => {
    openDialog.mockResolvedValueOnce(null);

    render(<ExportControls runId="saved-run" markdown="# r" />);
    await userEvent.setup().click(
      screen.getByRole("button", { name: "Export complete report" }),
    );

    expect(invoke).not.toHaveBeenCalled();
  });
});
