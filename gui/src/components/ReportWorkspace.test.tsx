import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ReportWorkspace, { splitUnexpectedPreamble } from "./ReportWorkspace";
import type { PipelineReport } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

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

  it("presents the report as the default flagship view with provenance", async () => {
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
    expect(screen.getByText("Monetary Policy and Networks")).toBeVisible();
    expect(
      await screen.findByRole("region", { name: "Run provenance" }),
    ).toBeVisible();
    expect(screen.getByText("gpt-5.6-sol")).toBeVisible();
    expect(await screen.findByRole("heading", { name: "Referee Report" })).toBeVisible();
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
