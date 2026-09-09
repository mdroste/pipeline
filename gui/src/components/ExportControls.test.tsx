import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ExportControls from "./ExportControls";
import type { PipelineReport } from "../lib/types";

const openDialog = vi.hoisted(() => vi.fn());
const invoke = vi.hoisted(() => vi.fn());
const confirmDialog = vi.hoisted(() => vi.fn());
const notify = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialog }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./DialogService", () => ({ confirmDialog, notify }));

const fakeReport: PipelineReport = {
  orientation: {},
  step_outputs: [],
  report_date: "2026-08-23",
  paper_hash: "abc",
};

async function openMenu() {
  await userEvent.setup().click(screen.getByRole("button", { name: /Export/ }));
}

describe("ExportControls", () => {
  beforeEach(() => {
    openDialog.mockReset();
    invoke.mockReset();
    confirmDialog.mockReset();
    notify.mockReset();
  });

  it("uses one export menu and accurately labels the print-dialog flow", async () => {
    render(<ExportControls markdown="# report" />);
    expect(screen.getAllByRole("button")).toHaveLength(1);
    await openMenu();
    expect(
      screen.getByRole("menuitem", { name: "Save Markdown…" }),
    ).toBeVisible();
    expect(
      screen.getByRole("menuitem", { name: "Print / save as PDF…" }),
    ).toBeVisible();
  });

  it("saves Markdown without accepting a webview-controlled path", async () => {
    invoke.mockResolvedValueOnce("/tmp/report.md");
    render(<ExportControls markdown="# report" />);
    await openMenu();
    await userEvent
      .setup()
      .click(screen.getByRole("menuitem", { name: "Save Markdown…" }));
    const [command, args] = invoke.mock.calls[0];
    expect(command).toBe("save_report_md");
    expect(args.markdown).toBe("# report");
    expect(args.suggestedName).toMatch(/^PIPELINE_REPORT_.*\.md$/);
    expect(args).not.toHaveProperty("path");
  });

  it("opens the native print view with clean content and provenance", async () => {
    invoke.mockResolvedValueOnce(undefined);
    render(
      <ExportControls
        markdown="# raw"
        pdfMarkdown="## Clean"
        provenanceMarkdown="# Provenance"
      />,
    );
    await openMenu();
    await userEvent
      .setup()
      .click(screen.getByRole("menuitem", { name: "Print / save as PDF…" }));
    expect(invoke).toHaveBeenCalledWith("print_report_html", {
      markdown: "## Clean",
      provenanceMarkdown: "# Provenance",
    });
  });

  it("offers safe, forensic, and custom packages only for saved runs", async () => {
    render(<ExportControls runId="saved-run" markdown="# report" />);
    await openMenu();
    expect(
      screen.getByRole("menuitem", { name: /Shareable report/ }),
    ).toHaveTextContent("No source or raw responses");
    expect(
      screen.getByRole("menuitem", { name: /Forensic archive/ }),
    ).toHaveTextContent("Sensitive");
    expect(
      screen.getByRole("menuitem", { name: "Custom selection…" }),
    ).toBeVisible();
  });

  it("exports the safe shareable selection by default and displays its checksum", async () => {
    openDialog.mockResolvedValueOnce("/tmp/exports");
    invoke.mockResolvedValueOnce({
      exportedPath: "/tmp/exports/pipeline-shareable-saved-run",
      fileCount: 7,
      bytes: 2048,
      checksum: "a".repeat(64),
      mode: "shareable",
      sensitivity: "shareable",
    });
    render(<ExportControls runId="saved-run" markdown="# report" />);
    await openMenu();
    await userEvent
      .setup()
      .click(screen.getByRole("menuitem", { name: /Shareable report/ }));
    expect(confirmDialog).not.toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("export_run_package", {
      runId: "saved-run",
      destination: "/tmp/exports",
      mode: "shareable",
      selection: {
        report: true,
        verifiedFindings: true,
        provenance: true,
        workflow: false,
        sourceDocuments: false,
        rawResponses: false,
        logs: false,
        supportingArtifacts: false,
      },
    });
    expect(await screen.findByText(/shareable package ready/i)).toBeVisible();
    expect(screen.getByText(/SHA-256 a{64}/)).toBeVisible();
  });

  it("requires explicit confirmation for a forensic archive", async () => {
    confirmDialog.mockResolvedValueOnce(false);
    render(<ExportControls runId="saved-run" markdown="# report" />);
    await openMenu();
    await userEvent
      .setup()
      .click(screen.getByRole("menuitem", { name: /Forensic archive/ }));
    expect(confirmDialog.mock.calls[0][0]).toMatch(
      /source documents, raw model responses, logs/i,
    );
    expect(openDialog).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("marks sensitive custom choices and confirms them", async () => {
    confirmDialog.mockResolvedValueOnce(false);
    render(<ExportControls runId="saved-run" markdown="# report" />);
    await openMenu();
    await userEvent
      .setup()
      .click(screen.getByRole("menuitem", { name: "Custom selection…" }));
    await userEvent
      .setup()
      .click(
        screen.getByRole("checkbox", { name: "Source documents · Sensitive" }),
      );
    expect(screen.getByText(/selection is sensitive/i)).toBeVisible();
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Export selected items…" }));
    expect(confirmDialog).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("reveals only the exact completed package path returned by the backend", async () => {
    openDialog.mockResolvedValueOnce("/tmp/exports");
    invoke
      .mockResolvedValueOnce({
        exportedPath: "/tmp/exports/package",
        fileCount: 1,
        bytes: 10,
        checksum: "abc",
        mode: "shareable",
        sensitivity: "shareable",
      })
      .mockResolvedValueOnce(undefined);
    render(<ExportControls runId="saved-run" markdown="# report" />);
    await openMenu();
    await userEvent
      .setup()
      .click(screen.getByRole("menuitem", { name: /Shareable report/ }));
    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Reveal in folder" }));
    expect(invoke).toHaveBeenLastCalledWith("reveal_export_in_folder", {
      path: "/tmp/exports/package",
    });
  });

  it("keeps an unsaved core package inside the same menu", async () => {
    openDialog.mockResolvedValueOnce("/tmp/core");
    invoke.mockResolvedValueOnce(undefined);
    render(
      <ExportControls
        markdown="# report"
        report={fakeReport}
        extractedText="text"
      />,
    );
    await openMenu();
    await userEvent
      .setup()
      .click(
        screen.getByRole("menuitem", { name: "Export unsaved core package…" }),
      );
    expect(invoke).toHaveBeenCalledWith("save_all_artifacts", {
      dir: "/tmp/core",
      markdown: "# report",
      extractedText: "text",
      report: fakeReport,
    });
  });
});
