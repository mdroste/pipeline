import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";

const invoke = vi.hoisted(() => vi.fn());
const startPipeline = vi.hoisted(() => vi.fn());
const onCloseRequested = vi.hoisted(() => vi.fn());
const workflowInputMode = vi.hoisted(() => ({ value: "document" }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onCloseRequested }),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: () => Promise.resolve("test"),
}));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
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
  default: ({
    inputMode,
    onPathChange,
  }: {
    inputMode: string;
    onPathChange: (path: string) => void;
  }) => (
    <>
      <span>Primary mode: {inputMode}</span>
      <button onClick={() => onPathChange("/tmp/test-paper.pdf")}>Choose test paper</button>
      <button onClick={() => onPathChange("/tmp/test-paper.docx")}>Choose DOCX</button>
    </>
  ),
}));
vi.mock("./components/WorkflowPanel", () => ({
  default: () => <div>Test workflow</div>,
}));
vi.mock("./components/HistoryPage", () => ({
  default: () => <div>History workspace</div>,
}));
vi.mock("./components/PipelinePage", () => ({
  default: ({
    onClose,
    onDirtyChange,
    onProfileChange,
  }: {
    onClose: () => void;
    onDirtyChange?: (dirty: boolean) => void;
    onProfileChange?: (config?: {
      extraction: { input_mode: string };
    }) => void;
  }) => (
    <div>
      Workflow workspace
      <button onClick={() => onDirtyChange?.(true)}>Make workflow dirty</button>
      <button
        onClick={() => {
          workflowInputMode.value = "document";
          onProfileChange?.({ extraction: { input_mode: "document" } });
        }}
      >
        Save document workflow
      </button>
      <button
        onClick={() => {
          workflowInputMode.value = "folder";
          onProfileChange?.({ extraction: { input_mode: "folder" } });
        }}
      >
        Switch to folder workflow
      </button>
      <button onClick={onClose}>Close workflow editor</button>
    </div>
  ),
}));
vi.mock("./components/SettingsPage", () => ({
  default: ({
    onDirtyChange,
    onSystemChange,
  }: {
    onDirtyChange?: (dirty: boolean) => void;
    onSystemChange?: () => void;
  }) => (
    <div>
      Settings workspace
      <button onClick={() => onDirtyChange?.(true)}>Make settings dirty</button>
      <button onClick={() => onSystemChange?.()}>Save mocked settings</button>
    </div>
  ),
}));
vi.mock("./components/UpdateBanner", () => ({ default: () => null }));

describe("App run options", () => {
  beforeEach(() => {
    invoke.mockReset();
    startPipeline.mockReset();
    onCloseRequested.mockReset();
    onCloseRequested.mockResolvedValue(vi.fn());
    workflowInputMode.value = "document";
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
      if (command === "get_execution_plan") {
        return Promise.resolve({
          profileId: "deep-review",
          profileConfigSnapshotId: "config-test",
          profileSnapshotId: "snapshot-test",
          inputMode: workflowInputMode.value,
          variables: [],
          inputSlots: [],
          readiness: { deps: [], ready: true },
          stages: [
            { id: "extracting", kind: "extracting", label: "Extract input", stepIds: [] },
            { id: "done", kind: "done", label: "Complete", stepIds: [] },
          ],
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
      expect(invoke).toHaveBeenCalledWith("get_execution_plan", {
        variables: null,
        extraInputs: null,
        expectedProfileConfigSnapshotId: "config-test",
        diff: false,
        paperPath: "/tmp/test-paper.pdf",
      });
      expect(startPipeline).toHaveBeenLastCalledWith(
        "/tmp/test-paper.pdf",
        false,
        undefined,
        undefined,
        "snapshot-test",
      );
      expect(startPipeline).toHaveBeenCalledTimes(1);
    });
  });

  it("rechecks readiness for DOCX so a conservative extractor failure does not block the run", async () => {
    invoke.mockImplementation((command: string, args?: { paperPath?: string | null }) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_execution_plan") {
        const exactInput = args?.paperPath === "/tmp/test-paper.docx";
        return Promise.resolve({
          profileId: "document-review",
          profileConfigSnapshotId: "doc-config",
          profileSnapshotId: "doc-runtime",
          inputMode: "document",
          variables: [],
          inputSlots: [],
          readiness: exactInput
            ? { deps: [], ready: true }
            : {
                ready: false,
                deps: [{
                  name: "PDF extractor",
                  found: false,
                  version: "",
                  path: "",
                  required: true,
                  hint: "Only needed for PDF inputs.",
                }],
              },
          stages: [],
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Choose DOCX" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Run" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Run" })).toBeEnabled());
    expect(invoke.mock.calls.filter(
      ([command, args]) =>
        command === "get_execution_plan" &&
        (args as { paperPath?: string } | undefined)?.paperPath === "/tmp/test-paper.docx",
    ).length).toBeGreaterThanOrEqual(2);
    await user.click(screen.getByRole("button", { name: "Close" }));
    await user.click(screen.getByRole("button", { name: "Run" }));

    await waitFor(() => expect(startPipeline).toHaveBeenCalledWith(
      "/tmp/test-paper.docx",
      false,
      undefined,
      undefined,
      "doc-runtime",
    ));
  });

  it("retains an exact DOCX readiness check after a same-mode workflow save", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Choose DOCX" }));
    await user.click(screen.getByRole("button", { name: "Workflows" }));
    await user.click(await screen.findByRole("button", {
      name: "Save document workflow",
    }));
    await user.click(screen.getByRole("button", {
      name: "Close workflow editor",
    }));

    await waitFor(() => {
      const exactChecks = invoke.mock.calls.filter(
        ([command, args]) =>
          command === "get_execution_plan" &&
          (args as { paperPath?: string } | undefined)?.paperPath ===
            "/tmp/test-paper.docx",
      );
      expect(exactChecks.length).toBeGreaterThanOrEqual(2);
      expect(screen.getByRole("button", { name: "Run" })).toBeEnabled();
    });
  });

  it("clears an incompatible primary selection when the workflow input mode changes", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Choose test paper" }));
    await user.click(screen.getByRole("button", { name: "Workflows" }));
    await user.click(await screen.findByRole("button", {
      name: "Switch to folder workflow",
    }));
    await user.click(screen.getByRole("button", {
      name: "Close workflow editor",
    }));

    expect(await screen.findByText("Primary mode: folder")).toBeVisible();
    expect(screen.getByRole("button", { name: "Run" })).toBeDisabled();
    expect(invoke).toHaveBeenLastCalledWith("get_execution_plan", expect.objectContaining({
      expectedProfileConfigSnapshotId: null,
      paperPath: null,
    }));
  });

  it("reloads the new settings snapshot without comparing it to the old fingerprint", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: "Run" });

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", {
      name: "Save mocked settings",
    }));

    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("get_execution_plan", {
        variables: null,
        extraInputs: null,
        expectedProfileConfigSnapshotId: null,
        diff: false,
        paperPath: null,
      }),
    );
  });

  it("fails closed when fresh profile readiness reports a secondary provider missing", async () => {
    let checks = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_execution_plan") {
        checks += 1;
        return Promise.resolve({
          profileId: "mixed-providers",
          profileConfigSnapshotId: "mixed-config",
          profileSnapshotId: "mixed-runtime",
          inputMode: "document",
          variables: [],
          inputSlots: [],
          readiness: checks === 1
            ? { deps: [], ready: true }
            : {
                ready: false,
                deps: [{
                  name: "Gemini CLI",
                  found: false,
                  version: "",
                  path: "",
                  required: true,
                  hint: "Required by the explicit empirical agent.",
                }],
              },
          stages: [],
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Choose test paper" }));
    await user.click(screen.getByRole("button", { name: "Run" }));

    expect(await screen.findByRole("dialog", { name: "Dependencies" })).toBeVisible();
    expect(screen.getByText("Required dependencies missing.")).toBeVisible();
    expect(startPipeline).not.toHaveBeenCalled();
    expect(checks).toBe(2);
  });

  it("rejects setup when the active profile changes before launch", async () => {
    let plans = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_execution_plan") {
        plans += 1;
        if (plans > 1) return Promise.reject(new Error("active profile changed"));
        return Promise.resolve({
          profileId: "first",
          profileConfigSnapshotId: "first-config",
          profileSnapshotId: "first-runtime",
          inputMode: "document",
          variables: [],
          inputSlots: [],
          readiness: { deps: [], ready: true },
          stages: [],
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Choose test paper" }));
    await user.click(screen.getByRole("button", { name: "Run" }));

    expect((await screen.findAllByText("active profile changed")).length).toBeGreaterThan(0);
    expect(startPipeline).not.toHaveBeenCalled();
  });

  it("shows run setup only in the new-run workspace", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(screen.getByTestId("run-setup-panel")).toBeVisible();
    expect(screen.getByRole("button", { name: "New run" })).toHaveAttribute(
      "aria-current",
      "page",
    );

    await user.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("History workspace")).toBeVisible();
    expect(screen.queryByTestId("run-setup-panel")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "History" })).toHaveAttribute(
      "aria-current",
      "page",
    );

    await user.click(screen.getByRole("button", { name: "New run" }));
    expect(screen.getByTestId("run-setup-panel")).toBeVisible();
  });

  it("shows the first-run data notice once and remembers acknowledgement", async () => {
    const user = userEvent.setup();
    const firstRender = render(<App />);

    expect(screen.getByRole("note", { name: "Data and privacy" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Got it" }));
    expect(screen.queryByRole("note", { name: "Data and privacy" })).not.toBeInTheDocument();
    await screen.findByRole("button", { name: "Run" });

    firstRender.unmount();
    render(<App />);
    expect(screen.queryByRole("note", { name: "Data and privacy" })).not.toBeInTheDocument();
    await screen.findByRole("button", { name: "Run" });
  });

  it("links the first-run notice to the detailed privacy explanation", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Privacy details" }));
    expect(
      await screen.findByRole("heading", { name: "Data and privacy" }),
    ).toBeVisible();
  });

  it("blocks in-app navigation away from unsaved settings", async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: "Make settings dirty" }));
    await user.click(screen.getByRole("button", { name: "History" }));

    expect(confirmSpy).toHaveBeenCalledWith(
      "You have unsaved settings changes. Leave and discard them?",
    );
    expect(screen.getByText("Settings workspace")).toBeVisible();
    expect(screen.queryByText("History workspace")).not.toBeInTheDocument();
    confirmSpy.mockRestore();
  });

  it("blocks native window close while workflow changes are unsaved", async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<App />);

    await waitFor(() => expect(onCloseRequested).toHaveBeenCalledTimes(1));
    await user.click(screen.getByRole("button", { name: "Workflows" }));
    await user.click(await screen.findByRole("button", { name: "Make workflow dirty" }));

    const event = { preventDefault: vi.fn() };
    const closeHandler = onCloseRequested.mock.calls[0][0] as (
      closeEvent: { preventDefault: () => void },
    ) => void;
    closeHandler(event);

    expect(confirmSpy).toHaveBeenCalledWith(
      "You have unsaved changes. Quit and discard them?",
    );
    expect(event.preventDefault).toHaveBeenCalledTimes(1);
    confirmSpy.mockRestore();
  });

  it("warns when native window-close protection cannot be registered", async () => {
    onCloseRequested.mockRejectedValueOnce(new Error("window API unavailable"));
    render(<App />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Window-close protection is unavailable",
    );
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeVisible();
  });
});
