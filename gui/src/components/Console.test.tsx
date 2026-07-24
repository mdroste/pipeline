import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Console from "./Console";
import type { LlmRequestDetails, LogEntry, UsageState } from "../hooks/usePipeline";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const EMPTY_USAGE: UsageState = {
  total: { input: 0, output: 0, cached: 0, cacheWrite: 0 },
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

  it("surfaces cache reads and warm-up tokens", () => {
    render(
      <Console
        logs={[]}
        usage={{
          total: { input: 50_000, output: 2_000, cached: 40_000, cacheWrite: 8_000 },
          bySession: {},
        }}
      />
    );
    expect(screen.getByText(/40k cached/)).toBeInTheDocument();
    expect(screen.getByText(/8k warmed/)).toBeInTheDocument();
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
});
