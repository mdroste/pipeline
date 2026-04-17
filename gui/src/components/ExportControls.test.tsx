import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ExportControls from "./ExportControls";
import type { PipelineReport } from "../lib/types";

const save = vi.hoisted(() => vi.fn());
const openDialog = vi.hoisted(() => vi.fn());
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ save, open: openDialog }));
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
    save.mockReset();
    openDialog.mockReset();
    invoke.mockReset();
  });

  it("hides the 'Save All' button when report or extractedText is missing", () => {
    render(<ExportControls markdown="# hi" />);
    expect(screen.getByRole("button", { name: "Save MD" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save PDF" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Save All" })).not.toBeInTheDocument();
  });

  it("shows 'Save All' when report and extractedText are both provided", () => {
    render(<ExportControls markdown="# hi" report={fakeReport} extractedText="text" />);
    expect(screen.getByRole("button", { name: "Save All" })).toBeInTheDocument();
  });

  it("invokes save_report_md with the chosen path and markdown", async () => {
    save.mockResolvedValueOnce("/tmp/out.md");
    invoke.mockResolvedValueOnce(undefined);

    render(<ExportControls markdown="# report" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save MD" }));

    expect(save).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("save_report_md", {
      path: "/tmp/out.md",
      markdown: "# report",
    });
  });

  it("does not invoke the backend when the save dialog is cancelled", async () => {
    save.mockResolvedValueOnce(null);

    render(<ExportControls markdown="# report" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save MD" }));

    expect(invoke).not.toHaveBeenCalled();
  });

  it("invokes print_report_html when 'Save PDF' is clicked", async () => {
    invoke.mockResolvedValueOnce(undefined);

    render(<ExportControls markdown="# report" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save PDF" }));

    expect(invoke).toHaveBeenCalledWith("print_report_html", { markdown: "# report" });
  });

  it("alerts with the backend error message on save failure", async () => {
    save.mockResolvedValueOnce("/tmp/out.md");
    invoke.mockRejectedValueOnce(new Error("disk full"));
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});

    render(<ExportControls markdown="# report" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save MD" }));

    expect(alertSpy).toHaveBeenCalledWith("Failed to save: disk full");
    alertSpy.mockRestore();
  });

  it("invokes save_all_artifacts with the chosen directory", async () => {
    openDialog.mockResolvedValueOnce("/tmp/artifacts");
    invoke.mockResolvedValueOnce(undefined);
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});

    render(<ExportControls markdown="# r" report={fakeReport} extractedText="text" />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Save All" }));

    expect(openDialog).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
      title: "Choose folder for artifacts",
    });
    expect(invoke).toHaveBeenCalledWith("save_all_artifacts", {
      dir: "/tmp/artifacts",
      markdown: "# r",
      extractedText: "text",
      report: fakeReport,
    });
    expect(alertSpy).toHaveBeenCalledWith("All artifacts saved.");
    alertSpy.mockRestore();
  });
});
