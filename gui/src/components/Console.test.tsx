import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Console from "./Console";
import type { LlmRequestDetails, LogEntry, UsageState } from "../hooks/usePipeline";

const NO_TOOL_CALLS = {
  text_file: 0,
  image: 0,
  web: 0,
  shell_or_other: 0,
  unknown: 0,
};

const EMPTY_USAGE: UsageState = {
  total: {
    input: 0,
    output: 0,
    cached: 0,
    cacheWrite: 0,
    modelRoundTrips: 0,
    toolCalls: NO_TOOL_CALLS,
  },
  bySession: {},
};

function log(line: string, level = "info", session: number | null = null): LogEntry {
  return { line, level, session, label: null, t: 1_700_000_000_000 };
}

const REQUEST: LlmRequestDetails = {
  provider: "claude",
  provider_label: "Anthropic",
  transport: "api",
  model: "claude-sonnet-4-6",
  model_policy: "Balanced",
  effort: "high",
  tools: ["Read"],
  timeout_secs: 600,
  max_output_tokens: 32_000,
  output_format: "text",
  prompt: "Transcribe the attached paper.",
  prompt_chars: 30,
  system_prompt: "Return Markdown.",
  shared_context: null,
  shared_context_chars: 0,
  pdf_attached: true,
  write_enabled: false,
  working_directory: null,
  read_directories: ["/papers"],
  write_directory: null,
  local_endpoint: null,
};

describe("Console", () => {
  it("collapses when its active run reaches a terminal state", () => {
    const { rerender } = render(<Console logs={[log("done")]} usage={EMPTY_USAGE} active />);
    expect(screen.getByRole("separator", { name: "Resize console" })).toBeVisible();
    rerender(<Console logs={[log("done")]} usage={EMPTY_USAGE} active={false} />);
    expect(screen.queryByRole("separator", { name: "Resize console" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Completed run console/ })).toBeVisible();
  });

  it("surfaces clipboard failures instead of reporting a false success", async () => {
    const user = userEvent.setup();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
    });
    Object.defineProperty(document, "execCommand", {
      configurable: true,
      value: vi.fn().mockReturnValue(false),
    });
    render(<Console logs={[log("copy me")]} usage={EMPTY_USAGE} />);
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Clipboard access failed");
    expect(screen.getByRole("button", { name: "Copy" })).toBeVisible();
    Reflect.deleteProperty(navigator, "clipboard");
    Reflect.deleteProperty(document, "execCommand");
  });

  it("renders all lines by default", () => {
    render(
      <Console
        logs={[log("hello world"), log("WARNING: careful", "warn"), log("ERROR: boom", "error")]}
        usage={EMPTY_USAGE}
      />
    );
    expect(screen.getByText("hello world")).toBeInTheDocument();
    expect(screen.getByText("WARNING: careful")).toBeInTheDocument();
    expect(screen.getByText("ERROR: boom")).toBeInTheDocument();
    expect(screen.getByText("3 lines")).toBeInTheDocument();
  });

  it("uses theme-aware chrome and readable log colors", () => {
    render(
      <Console
        logs={[
          log("info line"),
          log("WARNING: careful", "warn"),
          log("ERROR: boom", "error"),
          log("[stderr] diagnostic", "stderr"),
        ]}
        usage={EMPTY_USAGE}
      />,
    );

    expect(screen.getByTestId("console-panel")).toHaveClass(
      "console-panel",
      "bg-white",
      "dark:bg-gray-950",
      "border-gray-200",
      "dark:border-gray-800",
    );
    expect(screen.getByText("info line").parentElement).toHaveClass(
      "text-gray-700",
      "dark:text-gray-300",
    );
    expect(screen.getByText("WARNING: careful").parentElement).toHaveClass(
      "text-amber-600",
      "dark:text-yellow-400",
    );
    expect(screen.getByText("ERROR: boom").parentElement).toHaveClass(
      "text-red-600",
      "dark:text-red-400",
    );
    expect(screen.getByText("[stderr] diagnostic").parentElement).toHaveClass(
      "text-orange-600",
      "dark:text-orange-400",
    );
  });

  it("filters by search text", async () => {
    const user = userEvent.setup();
    render(<Console logs={[log("apple pie"), log("banana split")]} usage={EMPTY_USAGE} />);
    await user.type(screen.getByPlaceholderText("Search…"), "banana");
    expect(screen.queryByText("apple pie")).not.toBeInTheDocument();
    expect(screen.getByText("banana split")).toBeInTheDocument();
    expect(screen.getByText("1 lines")).toBeInTheDocument();
  });

  it("filters to errors only via the level chip", async () => {
    const user = userEvent.setup();
    render(
      <Console
        logs={[log("info line"), log("WARNING: w", "warn"), log("ERROR: e", "error")]}
        usage={EMPTY_USAGE}
      />
    );
    await user.click(screen.getByTitle("Errors only"));
    expect(screen.queryByText("info line")).not.toBeInTheDocument();
    expect(screen.queryByText("WARNING: w")).not.toBeInTheDocument();
    expect(screen.getByText("ERROR: e")).toBeInTheDocument();
  });

  it("shows an error count that reflects the whole log, not the filter", () => {
    render(
      <Console
        logs={[log("ok"), log("ERROR: one", "error"), log("ERROR: two", "error")]}
        usage={EMPTY_USAGE}
      />
    );
    // The jump-to-error control is labelled with the total error count.
    expect(screen.getByTitle("Scroll to the first error")).toHaveTextContent("2 errors");
  });

  it("bounds live DOM rows while preserving counts and error navigation", async () => {
    const user = userEvent.setup();
    const logs = [
      log("ERROR: earliest", "error"),
      ...Array.from({ length: 650 }, (_, index) => log(`line ${index}`)),
    ];
    render(<Console logs={logs} usage={EMPTY_USAGE} />);

    expect(screen.getByText("651 lines")).toBeVisible();
    expect(screen.getByText(/Showing 600 of 651 matching lines/)).toBeVisible();
    expect(screen.queryByText("ERROR: earliest")).not.toBeInTheDocument();
    expect(screen.getByText("line 649")).toBeVisible();

    await user.click(screen.getByTitle("Scroll to the first error"));
    expect(screen.getByText("ERROR: earliest")).toBeVisible();
    expect(screen.queryByText("line 649")).not.toBeInTheDocument();
  });

  it("decomposes logical input into fresh, cache-read, and cache-write tokens", () => {
    render(
      <Console
        logs={[]}
        usage={{
          total: {
            input: 50_000,
            output: 2_000,
            cached: 40_000,
            cacheWrite: 8_000,
            modelRoundTrips: 0,
            toolCalls: NO_TOOL_CALLS,
          },
          bySession: {},
        }}
      />
    );
    const summary = screen.getByLabelText(
      /50,000 logical input tokens equals 2,000 fresh input tokens plus 40,000 cache-read tokens plus 8,000 cache-write tokens/,
    );
    expect(summary).toHaveTextContent(
      "50k logical input = 2k fresh + 40k cache read + 8k cache write · 2k output",
    );
    expect(summary.getAttribute("title")).toContain(
      "Cache reads and cache writes are subsets of logical input, not additional tokens.",
    );
    expect(summary.getAttribute("title")).toContain(
      "Fresh input equals logical input minus cache reads minus cache writes.",
    );
    expect(summary).toHaveAttribute("tabindex", "0");
  });

  it("resizes vertically by dragging its upper border", () => {
    render(<Console logs={[log("hello")]} usage={EMPTY_USAGE} />);
    const handle = screen.getByRole("separator", { name: "Resize console" });
    const consoleElement = handle.parentElement;

    fireEvent.mouseDown(handle, { button: 0, clientY: 300 });
    fireEvent.mouseMove(document, { clientY: 200 });
    fireEvent.mouseUp(document);

    expect(consoleElement).toHaveStyle({ height: "292px" });
    expect(document.body.style.cursor).toBe("");
    expect(document.body.style.userSelect).toBe("");
  });

  it("supports keyboard resizing from the upper border", () => {
    render(<Console logs={[log("hello")]} usage={EMPTY_USAGE} />);
    const handle = screen.getByRole("separator", { name: "Resize console" });
    const consoleElement = handle.parentElement;

    fireEvent.keyDown(handle, { key: "ArrowUp" });
    expect(consoleElement).toHaveStyle({ height: "208px" });
    fireEvent.keyDown(handle, { key: "Home" });
    expect(consoleElement).toHaveStyle({ height: "96px" });
  });

  it("shows effective request settings and reveals the prompt for a selected session", async () => {
    const user = userEvent.setup();
    render(
      <Console
        logs={[
          {
            ...log("LLM request", "info", 7),
            label: "LLM PDF extraction",
            request: REQUEST,
          },
        ]}
        usage={EMPTY_USAGE}
      />
    );

    await user.selectOptions(screen.getByRole("combobox"), "7");
    expect(screen.getByText(/Anthropic \(claude\) · Direct API/)).toBeInTheDocument();
    expect(screen.getAllByText(/claude-sonnet-4-6/)).not.toHaveLength(0);
    expect(screen.getByText(/effort high · tools Read · timeout 600s/)).toBeInTheDocument();
    expect(screen.queryByText("Transcribe the attached paper.")).not.toBeInTheDocument();

    await user.click(screen.getByText(/View prompt/));
    expect(screen.getByText("Return Markdown.")).toBeInTheDocument();
    expect(screen.getByText("Transcribe the attached paper.")).toBeInTheDocument();
  });

  it("shows a CLI provider and transport without repeating CLI", () => {
    const cliRequest: LlmRequestDetails = {
      ...REQUEST,
      provider: "codex",
      provider_label: "Codex",
      transport: "cli",
      model: "gpt-5.6",
    };
    render(
      <Console
        logs={[
          {
            ...log("LLM request · Codex CLI", "info", 8),
            label: "Technical",
            request: cliRequest,
          },
        ]}
        usage={{
          total: {
            input: 50_000,
            output: 2_000,
            cached: 40_000,
            cacheWrite: 8_000,
            modelRoundTrips: 7,
            toolCalls: {
              text_file: 3,
              image: 1,
              web: 2,
              shell_or_other: 0,
              unknown: 1,
            },
          },
          bySession: {
            8: {
              input: 50_000,
              output: 2_000,
              cached: 40_000,
              cacheWrite: 8_000,
              modelRoundTrips: 7,
              toolCalls: {
                text_file: 3,
                image: 1,
                web: 2,
                shell_or_other: 0,
                unknown: 1,
              },
            },
          },
        }}
      />
    );

    const selector = screen.getByRole("combobox");
    expect(selector).toHaveTextContent("Technical · Codex CLI · gpt-5.6");
    expect(selector).not.toHaveTextContent("CLI CLI");
    expect(selector).toHaveTextContent(
      "50k logical input = 2k fresh + 40k cache read + 8k cache write → 2k output",
    );
    expect(selector).not.toHaveTextContent("reported tool calls");
    expect(selector).not.toHaveTextContent("reported model round trips");
    const summary = screen.getByLabelText(/50,000 logical input tokens/);
    expect(summary.getAttribute("title")).not.toContain("tool calls");
    expect(summary.getAttribute("title")).not.toContain("model round trips");
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });

  it("labels bounded request fields when their previews are truncated", async () => {
    const user = userEvent.setup();
    const truncatedRequest: LlmRequestDetails = {
      ...REQUEST,
      prompt: "Task preview…",
      prompt_chars: 48_000,
      prompt_truncated: true,
      system_prompt: "System preview…",
      system_prompt_chars: 24_000,
      system_prompt_truncated: true,
      shared_context: "Context preview…",
      shared_context_chars: 72_000,
      shared_context_truncated: true,
    };
    render(
      <Console
        logs={[
          {
            ...log("LLM request", "info", 9),
            label: "Bounded request",
            request: truncatedRequest,
          },
        ]}
        usage={EMPTY_USAGE}
      />,
    );

    await user.selectOptions(screen.getByRole("combobox"), "9");
    expect(screen.getByText("Preview truncated")).toBeVisible();
    await user.click(screen.getByText(/View prompt/));

    expect(screen.getAllByText("Preview truncated")).toHaveLength(3);
    expect(screen.getByText("System prompt (24,000 characters)")).toBeVisible();
    expect(screen.getByText("Shared context (72,000 characters)")).toBeVisible();
    expect(screen.getByText("Task prompt (48,000 characters)")).toBeVisible();
    expect(
      screen.getByText(/Copy prompt copies only the visible text/),
    ).toBeVisible();
  });
});
