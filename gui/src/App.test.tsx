import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";

const invoke = vi.hoisted(() => vi.fn());
const startPipeline = vi.hoisted(() => vi.fn());
const onCloseRequested = vi.hoisted(() => vi.fn());
const setWindowTheme = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
const confirmDialog = vi.hoisted(() => vi.fn());
const notify = vi.hoisted(() => vi.fn());
const workflowInputMode = vi.hoisted(() => ({ value: "document" }));
let systemIsDark = false;
let systemThemeListener: ((event: MediaQueryListEvent) => void) | undefined;

const previewConfig = {
  steps: [],
  merge: { enabled: false, prompt: "", agents: [] },
  context_cache: { enabled: false },
  use_orientation: true,
  orientation_prompt: "",
  extraction: { method: "", input_mode: "document", extra_inputs: [] },
  parallel_context_template: "{step_prompt}",
  variables: [],
};

function runSetup(
  profileId = "deep-review",
  profileConfigSnapshotId = "config-test",
) {
  return {
    profileId,
    profileConfigSnapshotId,
    inputMode: workflowInputMode.value,
    variables: [],
    inputSlots: [],
  };
}

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onCloseRequested, setTheme: setWindowTheme }),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: () => Promise.resolve("test"),
}));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("./components/DialogService", () => ({ confirmDialog, notify }));
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
    onSelectionChange,
  }: {
    inputMode: string;
    onPathChange: (path: string) => void;
    onSelectionChange?: (selection: {
      paths: string[];
      interpretation: "document" | "batch";
      selectionKind: "file";
    }) => void;
  }) => (
    <>
      <span>Primary mode: {inputMode}</span>
      <button onClick={() => {
        onPathChange("/tmp/test-paper.pdf");
        onSelectionChange?.({
          paths: ["/tmp/test-paper.pdf"],
          interpretation: "document",
          selectionKind: "file",
        });
      }}>Choose test paper</button>
      <button onClick={() => {
        onPathChange("/tmp/test-paper.docx");
        onSelectionChange?.({
          paths: ["/tmp/test-paper.docx"],
          interpretation: "document",
          selectionKind: "file",
        });
      }}>Choose DOCX</button>
      <button onClick={() => {
        onPathChange("/tmp/a.pdf");
        onSelectionChange?.({
          paths: ["/tmp/a.pdf", "/tmp/b.pdf"],
          interpretation: "batch",
          selectionKind: "file",
        });
      }}>Choose two papers</button>
    </>
  ),
}));
vi.mock("./components/WorkflowPanel", () => ({
  default: () => <div>Test workflow</div>,
}));
vi.mock("./components/RunParallelAgents", () => ({
  default: ({ onChange }: {
    onChange: (value: {
      agents: string[];
      model_overrides: Record<string, never>;
      effort_overrides: Record<string, never>;
    }) => void;
  }) => (
    <button onClick={() => onChange({
      agents: ["codex"],
      model_overrides: {},
      effort_overrides: {},
    })}>
      Change report agents
    </button>
  ),
}));
vi.mock("./components/HistoryPage", () => ({
  default: () => <div>History workspace</div>,
}));
vi.mock("./components/BatchPanel", () => ({
  default: ({
    preloadedSetup,
  }: {
    preloadedSetup?: { profileConfigSnapshotId?: string } | null;
  }) => (
    <div>
      Batch workspace · {preloadedSetup?.profileConfigSnapshotId ?? "no setup"}
    </div>
  ),
}));
vi.mock("./components/PipelinePage", () => ({
  default: ({
    onClose,
    onDirtyChange,
    onOpenGallery,
    onProfileChange,
  }: {
    onClose: () => void;
    onDirtyChange?: (dirty: boolean) => void;
    onOpenGallery?: () => void;
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
      <button onClick={onOpenGallery}>Gallery</button>
      <button onClick={onClose}>Close workflow editor</button>
    </div>
  ),
}));
vi.mock("./components/WorkflowGalleryPage", () => ({
  default: () => <div>Gallery workspace</div>,
}));
vi.mock("./components/SettingsPage", () => ({
  default: ({
    onDirtyChange,
    onSystemChange,
    theme,
    onThemeChange,
    initialSection,
    targetId,
  }: {
    onDirtyChange?: (dirty: boolean) => void;
    onSystemChange?: () => void;
    theme?: "light" | "dark" | "system";
    onThemeChange?: (theme: "light" | "dark" | "system") => void;
    initialSection?: string;
    targetId?: string;
  }) => (
    <div>
      <div>Settings workspace</div>
      <div>Initial settings section: {initialSection}</div>
      <div>Settings target: {targetId ?? "none"}</div>
      <div>Current theme: {theme}</div>
      <button onClick={() => onThemeChange?.("light")}>Use light theme</button>
      <button onClick={() => onThemeChange?.("dark")}>Use dark theme</button>
      <button onClick={() => onThemeChange?.("system")}>Use system theme</button>
      <button onClick={() => onDirtyChange?.(true)}>Make settings dirty</button>
      <button onClick={() => onSystemChange?.()}>Save mocked settings</button>
    </div>
  ),
}));
vi.mock("./components/UpdateBanner", () => ({ default: () => null }));

describe("App run options", () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
    confirmDialog.mockReset();
    notify.mockReset();
    listen.mockResolvedValue(vi.fn());
    startPipeline.mockReset();
    onCloseRequested.mockReset();
    onCloseRequested.mockResolvedValue(vi.fn());
    setWindowTheme.mockReset();
    setWindowTheme.mockResolvedValue(undefined);
    workflowInputMode.value = "document";
    systemIsDark = false;
    systemThemeListener = undefined;
    localStorage.clear();
    document.documentElement.classList.remove("dark", "theme-transitioning");
    document.documentElement.style.colorScheme = "";
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn(() => ({
        matches: systemIsDark,
        addEventListener: (_event: string, listener: (event: MediaQueryListEvent) => void) => {
          systemThemeListener = listener;
        },
        removeEventListener: (_event: string, listener: (event: MediaQueryListEvent) => void) => {
          if (systemThemeListener === listener) systemThemeListener = undefined;
        },
      })),
    });
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_pipeline_config") return Promise.resolve(previewConfig);
      if (command === "get_run_setup") return Promise.resolve(runSetup());
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
      if (command === "start_batch") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
  });

  it("supports light, dark, and live system appearance preferences", async () => {
    systemIsDark = true;
    const user = userEvent.setup();
    render(<App />);

    await waitFor(() => {
      expect(document.documentElement).toHaveClass("dark");
      expect(document.documentElement).not.toHaveClass("theme-transitioning");
      expect(setWindowTheme).toHaveBeenCalledWith(null);
    });

    await user.click(await screen.findByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: "Use light theme" }));
    expect(localStorage.getItem("theme")).toBe("light");
    expect(document.documentElement).not.toHaveClass("dark");
    expect(setWindowTheme).toHaveBeenLastCalledWith("light");

    act(() => systemThemeListener?.({ matches: true } as MediaQueryListEvent));
    expect(document.documentElement).not.toHaveClass("dark");

    await user.click(await screen.findByRole("button", { name: "Use system theme" }));
    expect(localStorage.getItem("theme")).toBe("system");
    expect(setWindowTheme).toHaveBeenLastCalledWith(null);
    expect(document.documentElement).toHaveClass("dark");

    act(() => systemThemeListener?.({ matches: false } as MediaQueryListEvent));
    expect(document.documentElement).not.toHaveClass("dark");
  });

  it("keeps revision reconciliation out of the main run controls", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(
      screen.queryByRole("checkbox", { name: /compare with previous run/i }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Choose test paper" }));
    await user.click(screen.getByRole("button", { name: "Review report" }));
    expect(await screen.findByRole("dialog", { name: "Review the execution plan" })).toBeVisible();
    expect(startPipeline).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Generate report" }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_execution_plan", {
        variables: null,
        extraInputs: null,
        expectedProfileConfigSnapshotId: "config-test",
        diff: false,
        paperPath: "/tmp/test-paper.pdf",
        inputInterpretation: "document",
      });
      expect(startPipeline).toHaveBeenLastCalledWith(
        "/tmp/test-paper.pdf",
        "document",
        false,
        undefined,
        undefined,
        "snapshot-test",
      );
      expect(startPipeline).toHaveBeenCalledTimes(1);
    });
  });

  it("keeps batch selection in New run instead of the left navigation", async () => {
    render(<App />);

    await screen.findByRole("button", { name: "Review report" });
    expect(screen.queryByRole("button", { name: "Batch" })).not.toBeInTheDocument();
  });

  it("opens Gallery from Workflows instead of the primary navigation", async () => {
    const user = userEvent.setup();
    render(<App />);

    await screen.findByRole("button", { name: "Review report" });
    expect(screen.queryByRole("button", { name: "Gallery" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Workflows" }));
    await user.click(await screen.findByRole("button", { name: "Gallery" }));

    expect(await screen.findByText("Gallery workspace")).toBeVisible();
    expect(screen.getByRole("button", { name: "Workflows" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("does not rebuild the execution plan when per-report agents change", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Choose test paper" }));
    await waitFor(() => expect(
      screen.getByRole("button", { name: "Review report" }),
    ).toBeEnabled());
    await waitFor(() => expect(invoke.mock.calls.filter(
      ([command]) => command === "get_execution_plan",
    )).toHaveLength(1));
    await user.click(screen.getByRole("button", { name: "Change report agents" }));

    expect(screen.getByRole("button", { name: "Review report" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Loading workflow…" })).not.toBeInTheDocument();
    expect(invoke.mock.calls.filter(
      ([command]) => command === "get_execution_plan",
    )).toHaveLength(1);
  });

  it("launches several selected documents as a batch from New run", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Choose two papers" }));
    await user.click(screen.getByRole("button", { name: "Review report" }));
    expect(await screen.findByText(/2 documents · document/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Generate report" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_batch", {
      paths: ["/tmp/a.pdf", "/tmp/b.pdf"],
      variables: null,
      extraInputs: null,
      expectedProfileConfigSnapshotId: "config-test",
    }));
    expect(await screen.findByText("Batch workspace · no setup")).toBeVisible();
    expect(screen.getByRole("button", { name: /Current batch/ })).toBeVisible();
    expect(screen.getByRole("button", { name: "New report" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("History workspace")).toBeVisible();
    await user.click(screen.getByRole("button", { name: /Current batch/ }));
    expect(await screen.findByText("Batch workspace · no setup")).toBeVisible();
    expect(startPipeline).not.toHaveBeenCalled();
  });

  it("defers exact DOCX readiness checks until the user reviews the report", async () => {
    invoke.mockImplementation((command: string, args?: { paperPath?: string | null }) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_pipeline_config") return Promise.resolve(previewConfig);
      if (command === "get_run_setup") {
        return Promise.resolve(runSetup("document-review", "doc-config"));
      }
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
    await waitFor(() => expect(screen.getByRole("button", { name: "Review report" })).toBeEnabled());
    expect(invoke.mock.calls.filter(
      ([command, args]) =>
        command === "get_execution_plan" &&
        (args as { paperPath?: string } | undefined)?.paperPath === "/tmp/test-paper.docx",
    )).toHaveLength(0);
    expect(screen.queryByRole("button", { name: "Refresh" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    await user.click(screen.getByRole("button", { name: "Review report" }));
    expect(await screen.findByRole("dialog", { name: "Review the execution plan" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Generate report" }));

    await waitFor(() => expect(startPipeline).toHaveBeenCalledWith(
      "/tmp/test-paper.docx",
      "document",
      false,
      undefined,
      undefined,
      "doc-runtime",
    ));
    expect(invoke.mock.calls.filter(
      ([command, args]) =>
        command === "get_execution_plan" &&
        (args as { paperPath?: string } | undefined)?.paperPath === "/tmp/test-paper.docx",
    )).toHaveLength(1);
  });

  it("does not rerun workflow preflight after a same-mode workflow save", async () => {
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

    await waitFor(() => expect(screen.getByRole("button", { name: "Review report" })).toBeEnabled());
    expect(invoke.mock.calls.filter(
      ([command, args]) =>
        command === "get_execution_plan" &&
        (args as { paperPath?: string } | undefined)?.paperPath === "/tmp/test-paper.docx",
    )).toHaveLength(0);
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
    expect(screen.getByRole("button", { name: "Review report" })).toBeDisabled();
    expect(invoke).toHaveBeenCalledWith("get_run_setup");
    expect(invoke.mock.calls.filter(
      ([command, args]) =>
        command === "get_execution_plan" &&
        (args as { paperPath?: string } | undefined)?.paperPath !== null,
    )).toHaveLength(0);
  });

  it("reloads the new settings snapshot without comparing it to the old fingerprint", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: "Review report" });

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", {
      name: "Save mocked settings",
    }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_execution_plan", {
        variables: null,
        extraInputs: null,
        expectedProfileConfigSnapshotId: null,
        diff: false,
        paperPath: null,
        inputInterpretation: null,
      }),
    );
  });

  it("does not cover Settings when the startup dependency check finishes late", async () => {
    let resolvePlan!: (plan: {
      profileId: string;
      profileConfigSnapshotId: string;
      profileSnapshotId: string;
      inputMode: string;
      variables: never[];
      inputSlots: never[];
      readiness: {
        ready: boolean;
        deps: Array<{
          name: string;
          found: boolean;
          version: string;
          path: string;
          required: boolean;
          hint: string;
        }>;
      };
      stages: never[];
    }) => void;
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_run_setup") {
        return Promise.resolve(runSetup("missing-provider", "missing-config"));
      }
      if (command === "get_execution_plan") {
        return new Promise((resolve) => {
          resolvePlan = resolve;
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByText("Settings workspace")).toBeVisible();

    await act(async () => {
      resolvePlan({
        profileId: "missing-provider",
        profileConfigSnapshotId: "missing-config",
        profileSnapshotId: "missing-runtime",
        inputMode: "document",
        variables: [],
        inputSlots: [],
        readiness: {
          ready: false,
          deps: [{
            name: "Claude CLI",
            found: false,
            version: "",
            path: "",
            required: true,
            hint: "Install or configure model access.",
          }],
        },
        stages: [],
      });
    });

    expect(screen.getByText("Settings workspace")).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "Dependencies" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Setup needed" })).toBeVisible();
  });

  it("opens PDF Extraction settings from a missing PaddleOCR dependency", async () => {
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_run_setup") {
        return Promise.resolve(runSetup("local-pdf", "local-pdf-config"));
      }
      if (command === "get_execution_plan") {
        return Promise.resolve({
          profileId: "local-pdf",
          profileConfigSnapshotId: "local-pdf-config",
          profileSnapshotId: "local-pdf-runtime",
          inputMode: "document",
          variables: [],
          inputSlots: [],
          readiness: {
            ready: false,
            deps: [{
              name: "PaddleOCR-VL Full Parser",
              found: false,
              version: "",
              path: "",
              required: true,
              hint: "Recommended for PDFs: Install from Settings → PDF Extraction.",
            }],
          },
          stages: [],
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("link", {
      name: "Settings → PDF Extraction",
    }));

    expect(screen.queryByRole("dialog", { name: "Dependencies" })).not.toBeInTheDocument();
    expect(await screen.findByText("Settings workspace")).toBeVisible();
    expect(screen.getByText("Initial settings section: extraction")).toBeVisible();
    expect(screen.getByText("Settings target: paddleocr-local-engine")).toBeVisible();
  });

  it("fails closed when fresh profile readiness reports a secondary provider missing", async () => {
    let checks = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_pipeline_config") return Promise.resolve(previewConfig);
      if (command === "get_run_setup") {
        return Promise.resolve(runSetup("mixed-providers", "mixed-config"));
      }
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
                  name: "Antigravity CLI",
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
    await user.click(screen.getByRole("button", { name: "Review report" }));

    expect(await screen.findByRole("dialog", { name: "Dependencies" })).toBeVisible();
    expect(screen.getByText(/Set up at least one model provider/)).toBeVisible();
    expect(startPipeline).not.toHaveBeenCalled();
    expect(checks).toBe(2);
  });

  it("rejects setup when the active profile changes before launch", async () => {
    let plans = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_pipeline_config") return Promise.resolve(previewConfig);
      if (command === "get_run_setup") {
        return Promise.resolve(runSetup("first", "first-config"));
      }
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
    await user.click(screen.getByRole("button", { name: "Review report" }));

    expect((await screen.findAllByText(/active profile changed/)).length).toBeGreaterThan(0);
    expect(startPipeline).not.toHaveBeenCalled();
  });

  it("shows run setup only in the new-run workspace", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(screen.getByTestId("run-setup-panel")).toBeVisible();
    expect(screen.getByRole("button", { name: "New report" })).toHaveAttribute(
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

    await user.click(screen.getByRole("button", { name: "New report" }));
    expect(screen.getByTestId("run-setup-panel")).toBeVisible();
  });

  it("shows the first-run data notice once and remembers acknowledgement", async () => {
    const user = userEvent.setup();
    const firstRender = render(<App />);

    expect(screen.getByRole("note", { name: "Data and privacy" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Got it" }));
    expect(screen.queryByRole("note", { name: "Data and privacy" })).not.toBeInTheDocument();
    await screen.findByRole("button", { name: "Review report" });

    firstRender.unmount();
    render(<App />);
    expect(screen.queryByRole("note", { name: "Data and privacy" })).not.toBeInTheDocument();
    await screen.findByRole("button", { name: "Review report" });
  });

  it("links the first-run notice to the detailed privacy explanation", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Privacy details" }));
    const summary = await screen.findByText("Data & privacy");
    expect(summary).toBeVisible();
    expect(summary.closest("details")).toHaveAttribute("open");
    expect(screen.getByText(/plan and data-use terms/i)).toBeVisible();
  });

  it("opens the PaddleOCR install card from the Help setup section", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Help" }));
    await user.click(await screen.findByRole("button", { name: "Install in Settings" }));

    expect(await screen.findByText("Settings workspace")).toBeVisible();
    expect(screen.getByText("Initial settings section: extraction")).toBeVisible();
    expect(screen.getByText("Settings target: paddleocr-local-engine")).toBeVisible();
  });

  it("blocks in-app navigation away from unsaved settings", async () => {
    const user = userEvent.setup();
    confirmDialog.mockResolvedValueOnce(false);
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: "Make settings dirty" }));
    await user.click(screen.getByRole("button", { name: "History" }));

    expect(confirmDialog).toHaveBeenCalledWith(
      "You have unsaved settings changes. Leave and discard them?",
      expect.anything(),
    );
    expect(screen.getByText("Settings workspace")).toBeVisible();
    expect(screen.queryByText("History workspace")).not.toBeInTheDocument();
  });

  it("blocks native window close while workflow changes are unsaved", async () => {
    const user = userEvent.setup();
    confirmDialog.mockResolvedValueOnce(false);
    render(<App />);

    await waitFor(() => expect(onCloseRequested).toHaveBeenCalledTimes(1));
    await user.click(screen.getByRole("button", { name: "Workflows" }));
    await user.click(await screen.findByRole("button", { name: "Make workflow dirty" }));

    const event = { preventDefault: vi.fn() };
    const closeHandler = onCloseRequested.mock.calls[0][0] as (
      closeEvent: { preventDefault: () => void },
    ) => void;
    closeHandler(event);

    expect(confirmDialog).toHaveBeenCalledWith(
      "You have unsaved changes. Quit and discard them?",
      expect.anything(),
    );
    expect(event.preventDefault).toHaveBeenCalledTimes(1);
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
