import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import HistoryPage from "./HistoryPage";
import type { RunSummary } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

function run(overrides: Partial<RunSummary>): RunSummary {
  return {
    run_id: "abc_run",
    created: "2026-07-23T12:00:00Z",
    input_name: "paper.pdf",
    input_path: "/papers/paper.pdf",
    input_mode: "document",
    profile_id: "deep-review",
    profile_name: "Deep Review",
    provider: "claude",
    status: "done",
    duration_secs: 60,
    input_tokens: 100,
    output_tokens: 20,
    cached_input_tokens: 0,
    cache_write_input_tokens: 0,
    step_count: 2,
    artifact_count: 4,
    failed_steps: [],
    resumable: false,
    title: "",
    tags: [],
    ...overrides,
  };
}

describe("HistoryPage", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("decomposes tokens without showing detailed model activity in the run list", async () => {
    const cachedRun = run({
      input_tokens: 50_000,
      output_tokens: 2_000,
      cached_input_tokens: 40_000,
      cache_write_input_tokens: 8_000,
      model_round_trips: 7,
      tool_calls: {
        text_file: 3,
        image: 1,
        web: 2,
        shell_or_other: 0,
        unknown: 1,
      },
    });
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") return Promise.resolve([cachedRun]);
      if (command === "runs_disk_usage") return Promise.resolve({ count: 1, bytes: 1024 });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    render(<HistoryPage onClose={vi.fn()} />);

    const summary = await screen.findByLabelText(
      /50,000 logical input tokens equals 2,000 fresh input tokens plus 40,000 cache-read tokens plus 8,000 cache-write tokens/,
    );
    expect(summary).toHaveTextContent(
      "50k logical input = 2k fresh + 40k cache read + 8k cache write · 2k output",
    );
    expect(summary.getAttribute("title")).toContain(
      "Cache reads and cache writes are subsets of logical input, not additional tokens.",
    );
    expect(summary).not.toHaveTextContent("reported model round trips");
    expect(summary).not.toHaveTextContent("reported tool calls");
    expect(summary.getAttribute("title")).not.toContain("model round trips");
    expect(summary.getAttribute("title")).not.toContain("tool calls");
    expect(summary).toHaveAttribute("tabindex", "0");
  });

  it("offers Resume only for runs with a durable restart point", async () => {
    const resumable = run({
      run_id: "cancelled_run",
      status: "cancelled",
      failed_steps: ["Run cancelled"],
      resumable: true,
    });
    const unrecoverable = run({
      run_id: "failed_run",
      status: "failed",
      failed_steps: ["Run failed"],
      resumable: false,
    });
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") return Promise.resolve([resumable, unrecoverable]);
      if (command === "runs_disk_usage") return Promise.resolve({ count: 2, bytes: 1024 });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const onRerun = vi.fn();
    render(<HistoryPage onClose={vi.fn()} onRerun={onRerun} />);

    const resume = await screen.findByRole("button", { name: "Resume" });
    expect(screen.getAllByText("paper.pdf")).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Resume" })).toHaveLength(1);

    await userEvent.setup().click(resume);
    expect(onRerun).toHaveBeenCalledWith("cancelled_run", true);
  });

  it("disables Resume and Regenerate while a run is already in progress", async () => {
    const resumable = run({
      run_id: "cancelled_run",
      status: "cancelled",
      failed_steps: ["Run cancelled"],
      resumable: true,
    });
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") return Promise.resolve([resumable]);
      if (command === "runs_disk_usage") return Promise.resolve({ count: 1, bytes: 1024 });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const onRerun = vi.fn();
    render(<HistoryPage onClose={vi.fn()} onRerun={onRerun} runInProgress />);

    const resume = await screen.findByRole("button", { name: "Resume" });
    const regenerate = screen.getByRole("button", { name: "Regenerate" });
    expect(resume).toBeDisabled();
    expect(regenerate).toBeDisabled();
    expect(resume).toHaveAttribute("title", "A report is already being generated");
    expect(regenerate).toHaveAttribute("title", "A report is already being generated");

    const user = userEvent.setup();
    await user.click(resume);
    await user.click(regenerate);
    expect(onRerun).not.toHaveBeenCalled();
  });

  it("orders selected comparisons chronologically, regardless of click order", async () => {
    const newer = run({
      run_id: "newer",
      title: "Newer run",
      created: "2026-07-24T12:00:00Z",
    });
    const older = run({
      run_id: "older",
      title: "Older run",
      created: "2026-07-20T12:00:00Z",
    });
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") return Promise.resolve([newer, older]);
      if (command === "runs_disk_usage") return Promise.resolve({ count: 2, bytes: 1024 });
      if (command === "get_run_report") return Promise.resolve({ step_outputs: [] });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<HistoryPage onClose={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "Compare" }));
    // Select the newer run first to reproduce the historical inversion.
    await user.click(screen.getByRole("checkbox", { name: "Select Newer run for comparison" }));
    await user.click(screen.getByRole("checkbox", { name: "Select Older run for comparison" }));
    await user.click(screen.getByRole("button", { name: "Compare selected" }));

    await waitFor(() => {
      const reportCalls = invoke.mock.calls.filter(([command]) => command === "get_run_report");
      expect(reportCalls).toEqual([
        ["get_run_report", { runId: "older" }],
        ["get_run_report", { runId: "newer" }],
      ]);
    });
  });

  it("ignores an older refresh that resolves after a newer one", async () => {
    const listResolvers: Array<(runs: RunSummary[]) => void> = [];
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") {
        return new Promise<RunSummary[]>((resolve) => listResolvers.push(resolve));
      }
      if (command === "runs_disk_usage") {
        return Promise.resolve({ count: 1, bytes: 1024 });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<HistoryPage onClose={vi.fn()} />);

    await waitFor(() => expect(listResolvers).toHaveLength(1));
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(listResolvers).toHaveLength(2));

    await act(async () => {
      listResolvers[1]([run({ run_id: "fresh", title: "Fresh result" })]);
    });
    expect(await screen.findByText("Fresh result")).toBeInTheDocument();

    await act(async () => {
      listResolvers[0]([run({ run_id: "stale", title: "Stale result" })]);
    });
    expect(screen.getByText("Fresh result")).toBeInTheDocument();
    expect(screen.queryByText("Stale result")).not.toBeInTheDocument();
  });
});
