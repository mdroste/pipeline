import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import HistoryPage from "./HistoryPage";
import type { RunSummary } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));

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
    listen.mockReset();
    listen.mockResolvedValue(() => {});
  });

  it("lists a run with a misquotation as done, not degraded", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs")
        return Promise.resolve([
          run({ run_id: "one", input_name: "one.pdf", status: "degraded" }),
          run({ run_id: "two", input_name: "two.pdf", status: "done" }),
          run({ run_id: "three", input_name: "three.pdf", status: "partial" }),
        ]);
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: 3, bytes: 300 });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<HistoryPage onClose={vi.fn()} />);

    await screen.findByText("one.pdf");
    expect(screen.queryByText("degraded")).not.toBeInTheDocument();
    const filter = screen.getByRole("combobox", { name: "Filter by status" });
    expect(
      within(filter).queryByRole("option", { name: "degraded" }),
    ).not.toBeInTheDocument();

    await user.selectOptions(filter, "done");
    expect(screen.getByText("one.pdf")).toBeVisible();
    expect(screen.getByText("two.pdf")).toBeVisible();
    expect(screen.queryByText("three.pdf")).not.toBeInTheDocument();
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
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: 1, bytes: 1024 });
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
      if (command === "list_runs")
        return Promise.resolve([resumable, unrecoverable]);
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: 2, bytes: 1024 });
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
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: 1, bytes: 1024 });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const onRerun = vi.fn();
    render(<HistoryPage onClose={vi.fn()} onRerun={onRerun} runInProgress />);

    const resume = await screen.findByRole("button", { name: "Resume" });
    const regenerate = screen.getByRole("button", { name: "Regenerate" });
    expect(resume).toBeDisabled();
    expect(regenerate).toBeDisabled();
    expect(resume).toHaveAttribute(
      "title",
      "A report is already being generated",
    );
    expect(regenerate).toHaveAttribute(
      "title",
      "A report is already being generated",
    );

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
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: 2, bytes: 1024 });
      if (command === "get_run_report")
        return Promise.resolve({ step_outputs: [] });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<HistoryPage onClose={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "Compare" }));
    // Select the newer run first to reproduce the historical inversion.
    await user.click(
      screen.getByRole("checkbox", { name: "Select Newer run for comparison" }),
    );
    await user.click(
      screen.getByRole("checkbox", { name: "Select Older run for comparison" }),
    );
    await user.click(screen.getByRole("button", { name: "Compare selected" }));

    await waitFor(() => {
      const reportCalls = invoke.mock.calls.filter(
        ([command]) => command === "get_run_report",
      );
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
        return new Promise<RunSummary[]>((resolve) =>
          listResolvers.push(resolve),
        );
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

  it("filters by status and provider, sorts by name, and paginates large histories", async () => {
    const runs = Array.from({ length: 27 }, (_, index) =>
      run({
        run_id: `run-${index}`,
        title: `Report ${String(index).padStart(2, "0")}`,
        provider: index % 2 === 0 ? "codex" : "claude",
        status: index % 3 === 0 ? "partial" : "done",
        created: `2026-08-${String((index % 23) + 1).padStart(2, "0")}T12:00:00Z`,
      }),
    );
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") return Promise.resolve(runs);
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: runs.length, bytes: 1024 });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<HistoryPage onClose={vi.fn()} />);

    expect(await screen.findByText("Page 1 of 2")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.getByText("Page 2 of 2")).toBeVisible();
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Filter by status" }),
      "partial",
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Filter by provider" }),
      "codex",
    );
    expect(screen.getByText("5 matching")).toBeVisible();
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Sort reports" }),
      "name",
    );
    const reportButtons = screen.getAllByRole("button", {
      name: /^Report \d+$/,
    });
    expect(reportButtons[0]).toHaveTextContent("Report 00");
  });

  it("marks running reviews and withholds actions the backend rejects", async () => {
    let listed = [
      run({
        run_id: "live_run",
        title: "Live review",
        status: "running",
        resumable: true,
      }),
      run({ run_id: "done_run", title: "Finished review" }),
    ];
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") return Promise.resolve(listed);
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: 2, bytes: 1024 });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<HistoryPage onClose={vi.fn()} onRerun={vi.fn()} />);

    expect(await screen.findByText("Live review")).toBeVisible();
    expect(screen.getByText("running", { selector: "span" })).toHaveClass(
      "bg-blue-100",
    );
    expect(
      screen.queryByRole("button", { name: "Live review" }),
    ).not.toBeInTheDocument();
    const [liveOpen, doneOpen] = screen.getAllByRole("button", {
      name: "Open",
    });
    expect(liveOpen).toBeDisabled();
    expect(doneOpen).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "Resume" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText("More actions for Live review"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByLabelText("More actions for Finished review"),
    ).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "running" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Compare" }));
    expect(
      screen.getByRole("checkbox", {
        name: "Select Live review for comparison",
      }),
    ).toBeDisabled();

    // When the queue reports the review finished, the list reloads.
    await waitFor(() => expect(listen).toHaveBeenCalled());
    const [event, onQueue] = listen.mock.calls[0];
    expect(event).toBe("runs:queue");
    listed = listed.map((entry) =>
      entry.run_id === "live_run" ? { ...entry, status: "done" } : entry,
    );
    await act(async () =>
      onQueue({
        payload: { limit: 2, runs: [{ runId: "live_run", status: "done" }] },
      }),
    );
    expect(
      await screen.findByRole("button", { name: "Live review" }),
    ).toBeEnabled();
  });

  it("starts each page and filter at the top and restores the list after a report", async () => {
    const runs = Array.from({ length: 27 }, (_, index) =>
      run({
        run_id: `run-${index}`,
        title: `Report ${String(index).padStart(2, "0")}`,
        created: `2026-08-${String((index % 23) + 1).padStart(2, "0")}T12:00:00Z`,
      }),
    );
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") return Promise.resolve(runs);
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: runs.length, bytes: 1024 });
      if (command === "get_run_report")
        return Promise.resolve({ step_outputs: [], failed_steps: [] });
      if (command === "read_artifact")
        return Promise.resolve({ text: "# Saved report" });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<HistoryPage onClose={vi.fn()} />);
    await screen.findByText("Page 1 of 2");
    const list = () =>
      screen
        .getAllByRole("button", { name: "Open" })[0]
        .closest(".overflow-auto") as HTMLElement;

    list().scrollTop = 900;
    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(list().scrollTop).toBe(0);

    list().scrollTop = 300;
    await user.type(
      screen.getByRole("textbox", { name: "Filter report history" }),
      "Report",
    );
    expect(list().scrollTop).toBe(0);

    list().scrollTop = 450;
    await user.click(screen.getAllByRole("button", { name: "Open" })[1]);
    await user.click(
      await screen.findByRole("button", { name: "Back to history" }),
    );
    await screen.findByText("Page 1 of 2");
    expect(list().scrollTop).toBe(450);
  });

  it("follows a cited source supplied after the run route has committed", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "list_runs") return Promise.resolve([run({})]);
      if (command === "runs_disk_usage")
        return Promise.resolve({ count: 1, bytes: 1024 });
      if (command === "get_run_report")
        return Promise.resolve({ step_outputs: [], failed_steps: [] });
      if (command === "read_artifact")
        return Promise.resolve({ text: "# Saved report" });
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const view = render(
      <HistoryPage onClose={vi.fn()} initialRunId="abc_run" />,
    );
    expect(await screen.findByRole("tab", { name: "Report" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    view.rerender(
      <HistoryPage
        onClose={vi.fn()}
        initialRunId="abc_run"
        initialSourceSelection={{ page: 12 }}
      />,
    );
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Sources" })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
  });
});
