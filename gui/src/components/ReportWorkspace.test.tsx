import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ReportWorkspace, { splitUnexpectedPreamble } from "./ReportWorkspace";
import type { PipelineReport } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

// Lazy workspace panels can take longer than Testing Library's one-second
// default to import on busy CI runners, especially the Intel macOS runner.
const lazyPanelWait = { timeout: 5_000 };

function makeReport(): PipelineReport {
  return {
    orientation: {
      metadata: {
        title: "Monetary Policy and Networks",
        authors: [],
        date: "",
        paper_type: "theory",
      },
      sections: [],
      formal_results: [],
      tables_figures: [],
      notation: [],
      stated_contribution: "",
      key_references: [],
      extraction_quality_notes: [],
    },
    step_outputs: [
      {
        step_id: "technical",
        step_label: "Technical",
        phase: "parallel",
        agent: "codex",
        provider: "codex",
        model: "gpt-5.6-sol",
        input_tokens: 1_200,
        output_tokens: 180,
        cached_input_tokens: 900,
        raw_text: "Technical output.",
      },
    ],
    failed_steps: [],
    report_date: "2026-07-23T19:00:00Z",
    paper_hash: "abcdef1234567890",
  };
}

describe("ReportWorkspace", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("presents the report by default and keeps provenance in its own tab", async () => {
    const user = userEvent.setup();
    render(
      <ReportWorkspace
        markdown={"# Referee Report\n\n## Summary\n\nText."}
        report={makeReport()}
        extractedText="Extracted paper."
        durationSecs={125}
      />,
    );

    expect(screen.getByRole("tab", { name: "Report" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "Report" })).toHaveAttribute(
      "aria-controls",
      "report-workspace-panel-report",
    );
    expect(screen.getByRole("tab", { name: "Report" })).toHaveAttribute(
      "tabindex",
      "0",
    );
    expect(screen.getByRole("tab", { name: "Sources" })).toHaveAttribute(
      "tabindex",
      "-1",
    );
    expect(await screen.findByRole("tabpanel", { name: "Report" }, lazyPanelWait)).toHaveAttribute(
      "aria-labelledby",
      "report-workspace-tab-report",
    );
    expect(screen.getByText("Monetary Policy and Networks")).toBeVisible();
    expect(screen.getByRole("tab", { name: "Provenance" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
    expect(screen.queryByRole("region", { name: "Run provenance" })).not.toBeInTheDocument();
    expect(screen.getByText("gpt-5.6-sol")).toBeVisible();
    expect(await screen.findByRole("heading", { name: "Referee Report" })).toBeVisible();

    await user.click(screen.getByRole("tab", { name: "Provenance" }));
    expect(await screen.findByRole("region", { name: "Run provenance" })).toBeVisible();
    expect(screen.getByRole("columnheader", { name: "Step" })).toBeVisible();
    expect(screen.getByRole("cell", { name: "Technical" })).toBeVisible();
    expect(screen.getAllByText("1,200")[0]).toBeVisible();
    expect(screen.getAllByText("180")[0]).toBeVisible();
    expect(screen.getAllByText("900")[0]).toBeVisible();
  });

  it("carries rounded seconds into minutes instead of displaying 60 seconds", async () => {
    const user = userEvent.setup();
    render(
      <ReportWorkspace
        markdown={"# Referee Report\n\nBody."}
        report={makeReport()}
        durationSecs={119.6}
      />,
    );

    await user.click(screen.getByRole("tab", { name: "Provenance" }));
    expect(screen.getAllByText("2m 0s")[0]).toBeVisible();
    expect(screen.queryByText("1m 60s")).not.toBeInTheDocument();
  });

  it("shows a separate token row for each model used by a step", async () => {
    const user = userEvent.setup();
    const report = makeReport();
    report.step_outputs[0].calls = [
      {
        provider: "codex",
        model: "gpt-5.6-sol",
        input_tokens: 700,
        output_tokens: 80,
      },
      {
        provider: "claude",
        model: "claude-opus-4-8",
        input_tokens: 500,
        output_tokens: 100,
      },
    ];

    render(
      <ReportWorkspace
        markdown={"# Referee Report\n\nBody."}
        report={report}
      />,
    );

    await user.click(screen.getByRole("tab", { name: "Provenance" }));
    expect(screen.getAllByRole("cell", { name: "Technical" })).toHaveLength(2);
    expect(screen.getByRole("cell", { name: "gpt-5.6-sol" })).toBeVisible();
    expect(screen.getByRole("cell", { name: "claude-opus-4-8" })).toBeVisible();
  });

  it("hides unexpected preamble by default and offers the raw output", async () => {
    const user = userEvent.setup();
    render(
      <ReportWorkspace
        markdown={"I will now prepare the requested report.\n\n# Referee Report\n\nBody."}
        report={makeReport()}
      />,
    );

    expect(await screen.findByRole("status")).toHaveTextContent("Output-quality warning");
    expect(screen.queryByText("I will now prepare the requested report.")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Raw output" }));
    expect(screen.getByText("I will now prepare the requested report.")).toBeVisible();
  });

  it("keeps technical material behind the Sources tab", async () => {
    const user = userEvent.setup();
    render(
      <ReportWorkspace
        markdown={"# Referee Report\n\nBody."}
        report={makeReport()}
        extractedText="Extracted paper."
      />,
    );

    expect(screen.queryByRole("button", { name: "Extracted text" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Sources" }));
    expect(
      await screen.findByRole("button", { name: "Extracted text" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Technical" })).toBeVisible();
  });

  it("activates and focuses available tabs with Arrow, Home, and End keys", async () => {
    const user = userEvent.setup();
    render(
      <ReportWorkspace
        markdown={"# Referee Report\n\nBody."}
        report={makeReport()}
        extractedText="Extracted paper."
      />,
    );

    const reportTab = screen.getByRole("tab", { name: "Report" });
    const provenanceTab = screen.getByRole("tab", { name: "Provenance" });
    const sourcesTab = screen.getByRole("tab", { name: "Sources" });
    reportTab.focus();

    await user.keyboard("{ArrowRight}");
    expect(provenanceTab).toHaveFocus();
    expect(provenanceTab).toHaveAttribute("aria-selected", "true");

    await user.keyboard("{ArrowRight}");
    expect(sourcesTab).toHaveFocus();
    expect(sourcesTab).toHaveAttribute("aria-selected", "true");
    expect(sourcesTab).toHaveAttribute("tabindex", "0");
    expect(reportTab).toHaveAttribute("tabindex", "-1");
    expect(await screen.findByRole("tabpanel", { name: "Sources" }, lazyPanelWait)).toHaveAttribute(
      "id",
      "report-workspace-panel-sources",
    );

    await user.keyboard("{ArrowLeft}");
    expect(provenanceTab).toHaveFocus();
    expect(provenanceTab).toHaveAttribute("aria-selected", "true");

    await user.keyboard("{ArrowLeft}");
    expect(reportTab).toHaveFocus();

    await user.keyboard("{End}");
    expect(sourcesTab).toHaveFocus();
    await user.keyboard("{Home}");
    expect(reportTab).toHaveFocus();
  });

  it("includes a conditional Issues tab in keyboard navigation order", async () => {
    const user = userEvent.setup();
    const report = makeReport();
    report.step_outputs[0].raw_text = JSON.stringify({
      issues: [{ id: "one", title: "First issue", severity: "high" }],
    });
    render(
      <ReportWorkspace
        markdown={"# Referee Report\n\nBody."}
        report={report}
      />,
    );

    const reportTab = screen.getByRole("tab", { name: "Report" });
    const provenanceTab = screen.getByRole("tab", { name: "Provenance" });
    const issuesTab = screen.getByRole("tab", { name: /Issues/ });
    const sourcesTab = screen.getByRole("tab", { name: "Sources" });
    expect(issuesTab).toHaveAttribute("id", "report-workspace-tab-issues");
    expect(issuesTab).toHaveAttribute(
      "aria-controls",
      "report-workspace-panel-issues",
    );

    reportTab.focus();
    await user.keyboard("{ArrowRight}");
    expect(provenanceTab).toHaveFocus();
    expect(provenanceTab).toHaveAttribute("aria-selected", "true");

    await user.keyboard("{ArrowRight}");
    expect(issuesTab).toHaveFocus();
    expect(issuesTab).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByRole("tabpanel", { name: /Issues/ }, lazyPanelWait)).toHaveAttribute(
      "aria-labelledby",
      "report-workspace-tab-issues",
    );

    await user.keyboard("{ArrowRight}");
    expect(sourcesTab).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(issuesTab).toHaveFocus();
  });

  it("loads a historical report into the same workspace", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "get_run_report") return Promise.resolve(makeReport());
      if (command === "read_artifact") {
        return Promise.resolve({ text: "# Historical Report\n\nSaved body." });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    render(<ReportWorkspace runId="saved_run" />);

    expect(
      await screen.findByRole("heading", { name: "Historical Report" }),
    ).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("get_run_report", { runId: "saved_run" });
    expect(invoke).toHaveBeenCalledWith("read_artifact", {
      runId: "saved_run",
      relPath: "report.md",
    });
    expect(
      screen.getByRole("button", { name: "Export complete run" }),
    ).toBeVisible();
  });

  it("keeps preserved agent reports reachable when canonical report files are malformed", async () => {
    const user = userEvent.setup();
    const responsePath = "artifacts/agent-responses/technical-codex-attempt-01-rejected-envelope.md";
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "get_run_manifest") {
        return Promise.resolve({
          run_id: "broken_run",
          created: "2026-08-10T12:00:00Z",
          input_path: "/papers/example.pdf",
          input_mode: "document",
          profile_name: "Referee report",
          provider: "codex",
          status: "partial",
          artifacts: [{
            rel_path: responsePath,
            label: "Technical · Codex · Attempt 1 · Rejected envelope",
            kind: "markdown",
            bytes: 120,
            sha256: "abcdef1234567890",
            group: "agent_response",
          }],
        });
      }
      if (command === "get_run_report") {
        return Promise.reject(new Error("Invalid report data"));
      }
      if (command === "read_artifact" && args?.relPath === "report.md") {
        return Promise.reject(new Error("Report artifact is malformed"));
      }
      if (command === "read_artifact" && args?.relPath === responsePath) {
        return Promise.resolve({
          kind: "markdown",
          bytes: 120,
          text: "# Useful agent report\n\nThe delimiter was missing, but the analysis survived.",
          base64: null,
          truncated: false,
          abs_path: `/runs/broken_run/${responsePath}`,
        });
      }
      if (command === "read_artifact" && args?.relPath === "context/document.md") {
        return Promise.reject(new Error("No readable document"));
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    render(<ReportWorkspace runId="broken_run" />);

    expect(await screen.findByText("Canonical report is incomplete.")).toBeVisible();
    expect(screen.getByRole("tab", { name: "Sources" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.queryByText("Could not open this report.")).not.toBeInTheDocument();
    const preserved = await screen.findByRole("button", {
      name: "Technical · Codex · Attempt 1 · Rejected envelope",
    });
    await user.click(preserved);
    expect(
      await screen.findByRole("heading", { name: "Useful agent report" }),
    ).toBeVisible();
    expect(screen.getByText(/analysis survived/)).toBeVisible();
  });

  it("preloads Sources metadata and the readable document for an open report", async () => {
    invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "get_run_manifest") {
        return Promise.resolve({ run_id: "saved_run", artifacts: [] });
      }
      if (command === "read_artifact" && args?.relPath === "context/document.md") {
        return Promise.resolve({
          kind: "markdown",
          bytes: 20,
          text: "# Readable document",
          base64: null,
          truncated: false,
          abs_path: "/runs/saved_run/context/document.md",
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    render(
      <ReportWorkspace
        runId="saved_run"
        markdown="# Report"
        report={makeReport()}
      />,
    );

    expect(invoke).toHaveBeenCalledWith("get_run_manifest", { runId: "saved_run" });
    expect(invoke).toHaveBeenCalledWith("read_artifact", {
      runId: "saved_run",
      relPath: "context/document.md",
    });
  });
});

describe("splitUnexpectedPreamble", () => {
  it("preserves normal reports and removes narration before the first title", () => {
    expect(splitUnexpectedPreamble("# Report\n\nBody.").preamble).toBe("");
    expect(
      splitUnexpectedPreamble("Here is the report.\n\n# Report\n\nBody."),
    ).toEqual({
      preamble: "Here is the report.",
      clean: "# Report\n\nBody.",
    });
  });

  it("moves embedded Pipeline run metadata out of the clean report", () => {
    const markdown = [
      "# Report",
      "",
      "<!-- PIPELINE RUN DETAILS START -->",
      "**LLM**: Codex · **Model**: gpt-5.6-sol",
      "<!-- PIPELINE RUN DETAILS END -->",
      "",
      "Body.",
    ].join("\n");

    expect(splitUnexpectedPreamble(markdown).clean).toBe("# Report\n\nBody.");
  });

  it("replaces the duplicated paper-title masthead with the report type", () => {
    const markdown = [
      "# Monetary Policy and Networks",
      "",
      "**Authors**: Ada Economist",
      "",
      "## Errors & Inconsistencies",
      "",
      "Finding.",
    ].join("\n");

    expect(
      splitUnexpectedPreamble(markdown, "Monetary Policy and Networks").clean,
    ).toBe("# Referee Report\n\n## Errors & Inconsistencies\n\nFinding.");
  });

  it("cleans legacy metadata and narration before the first report section", () => {
    const markdown = [
      "# Legacy Report",
      "",
      "**Authors**: A. Economist  ",
      "**Type**: theory · **Reviewed**: 2026-07-22  ",
      "**LLM**: Codex · **Model**: Automatic  ",
      "*Report generated by Pipeline*",
      "",
      "---",
      "",
      "I’ll read the prompt first, then draft the report.## Findings",
      "",
      "Substantive content.",
    ].join("\n");
    const result = splitUnexpectedPreamble(markdown);

    expect(result.preamble).toContain("I’ll read the prompt first");
    expect(result.clean).toBe(
      "# Legacy Report\n\n---\n\n## Findings\n\nSubstantive content.",
    );
    expect(result.clean).not.toContain("Report generated by Pipeline");
  });
});
