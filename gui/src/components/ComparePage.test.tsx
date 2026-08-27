import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ComparePage from "./ComparePage";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

function report(text: string) {
  return {
    orientation: {},
    step_outputs: [{
      step_id: "review",
      step_label: "Review",
      raw_text: text,
      phase: "parallel",
      agent: "codex",
      structured_json: false,
    }],
    report_date: "2026-08-23",
    paper_hash: "hash",
  };
}

function manifest(id: string, created: string, provider: string) {
  return {
    run_id: id,
    created,
    input_path: "/paper.pdf",
    input_mode: "document",
    profile_name: "Review",
    provider,
    artifacts: [{ rel_path: "report.md", label: "Report", kind: "markdown", bytes: 10, sha256: "abc", group: "report" }],
  };
}

describe("ComparePage", () => {
  beforeEach(() => invoke.mockReset());

  it("labels older/newer provenance, summarizes changes, and exports the diff", async () => {
    invoke.mockImplementation((command: string, args?: { runId?: string }) => {
      if (command === "get_run_report") return Promise.resolve(report(args?.runId === "older" ? "before" : "after"));
      if (command === "get_run_manifest") return Promise.resolve(args?.runId === "older"
        ? manifest("older", "2026-08-01T00:00:00Z", "claude")
        : manifest("newer", "2026-08-23T00:00:00Z", "codex"));
      if (command === "save_text_file") return Promise.resolve("/tmp/diff.md");
      return Promise.resolve(undefined);
    });
    const user = userEvent.setup();
    render(<ComparePage runA="older" runB="newer" onBack={vi.fn()} />);

    expect(await screen.findByRole("heading", { name: "Older report" })).toBeVisible();
    expect(screen.getByRole("heading", { name: "Newer report" })).toBeVisible();
    expect(screen.getByText(/1 changed · 0 added · 0 removed · 0 unchanged/)).toBeVisible();
    expect(screen.getByText(/Reconciliation sends both reports/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Export diff…" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_text_file", {
      content: expect.stringContaining("Older run: older"),
      suggestedName: "pipeline-diff-older-newer.md",
    }));
  });
});
