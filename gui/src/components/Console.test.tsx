import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Console from "./Console";
import type { LogEntry, UsageState } from "../hooks/usePipeline";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const EMPTY_USAGE: UsageState = { total: { input: 0, output: 0 }, bySession: {} };

function log(line: string, level = "info", session: number | null = null): LogEntry {
  return { line, level, session, label: null, t: 1_700_000_000_000 };
}

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
});
