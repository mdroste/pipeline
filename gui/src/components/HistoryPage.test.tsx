import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
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

describe("HistoryPage resume actions", () => {
  beforeEach(() => {
    invoke.mockReset();
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
});
