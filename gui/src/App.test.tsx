import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";

const invoke = vi.hoisted(() => vi.fn());
const startPipeline = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./hooks/usePipeline", () => ({
  usePipeline: () => ({
    state: { kind: "idle" },
    logs: [],
    usage: { total: {}, bySession: {} },
    startPipeline,
    rerunPipeline: vi.fn(),
    cancel: vi.fn(),
    reset: vi.fn(),
    listenersReady: true,
    runStartedAt: null,
    passTimes: {},
  }),
}));
vi.mock("./lib/platform", () => ({ isMac: false }));
vi.mock("./components/PaperSelector", () => ({
  default: ({ onPathChange }: { onPathChange: (path: string) => void }) => (
    <button onClick={() => onPathChange("/tmp/test-paper.pdf")}>Choose test paper</button>
  ),
}));
vi.mock("./components/WorkflowPanel", () => ({
  default: () => <div>Test workflow</div>,
}));
vi.mock("./components/UpdateBanner", () => ({ default: () => null }));
vi.mock("./components/ResizeHandle", () => ({ default: () => null }));

describe("App run options", () => {
  beforeEach(() => {
    invoke.mockReset();
    startPipeline.mockReset();
    localStorage.clear();
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn(() => ({
        matches: false,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    });
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "check_deps") return Promise.resolve({ deps: [], ready: true });
      if (command === "get_pipeline_config") {
        return Promise.resolve({
          extraction: { input_mode: "document", extra_inputs: [] },
          variables: [],
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
  });

  it("keeps revision reconciliation out of the main run controls", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(
      screen.queryByRole("checkbox", { name: /compare with previous run/i }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Choose test paper" }));
    await user.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => {
      expect(startPipeline).toHaveBeenLastCalledWith(
        "/tmp/test-paper.pdf",
        false,
        undefined,
        undefined,
      );
    });
  });
});
