import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { router } from "./lib/router";
import { queuedRun } from "./test/queuedRunFixture";
import {
  createAppTestFixture,
  type AppTestAppearance,
} from "./test/appShellFixtures";
const mocks = vi.hoisted(() => ({
  saveCollection: vi.fn(),
  invoke: vi.fn(),
  runQueue: { limit: 2, runs: [] as ReturnType<typeof queuedRun>[] },
  enqueue: vi.fn(),
  enqueueBatch: vi.fn(),
  onCloseRequested: vi.fn(),
  setWindowTheme: vi.fn(),
  listen: vi.fn(),
  confirmDialog: vi.fn(),
  notify: vi.fn(),
  workflowInputMode: { value: "document" },
}));
const { invoke, enqueue, enqueueBatch } = mocks;
const appearance: AppTestAppearance = {
  systemIsDark: false,
  systemThemeListener: undefined,
};
const { previewConfig, runSetup, reset } = createAppTestFixture(
  mocks,
  appearance,
);
// History and Designer sit inside Reviews, which stays closed until it is
// needed or the current page belongs to it.
async function openTools(user: {
  click: (element: Element) => Promise<unknown>;
}) {
  const expand = screen.queryByRole("button", { name: "Expand Reviews" });
  if (expand) await user.click(expand);
}

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: mocks.onCloseRequested,
    setTheme: mocks.setWindowTheme,
  }),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: () => Promise.resolve("test"),
}));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("./components/DialogService", () => ({
  confirmDialog: mocks.confirmDialog,
  notify: mocks.notify,
}));
vi.mock("./hooks/usePipeline", () => ({
  usePipeline: () => ({
    queue: { limit: mocks.runQueue.limit, runs: [...mocks.runQueue.runs] },
    views: {},
    listenersReady: true,
    setupError: null,
    enqueue: mocks.enqueue,
    enqueueBatch: mocks.enqueueBatch,
    enqueueRerun: vi.fn(),
    cancel: vi.fn(),
    dismiss: vi.fn(),
  }),
}));
vi.mock("./lib/platform", () => ({ isMac: false, isWindows: false }));
vi.mock("./components/PaperSelector", async () => {
  const { default: Fixture } = (
    await import("./test/appShellFixtures")
  ).createAppComponentMocks(mocks).PaperSelector;
  // Also shows the selection App holds, which must survive a remount.
  return {
    default: (
      props: Parameters<typeof Fixture>[0] & {
        selection?: { paths: string[] } | null;
      },
    ) => (
      <>
        <span>Shown input: {props.selection?.paths.join(", ") ?? "none"}</span>
        <Fixture {...props} />
      </>
    ),
  };
});
vi.mock(
  "./components/WorkflowPanel",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .WorkflowPanel,
);
vi.mock(
  "./components/RunParallelAgents",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .RunParallelAgents,
);
vi.mock(
  "./components/ProjectsPage",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .ProjectsPage,
);
vi.mock(
  "./components/HistoryPage",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .HistoryPage,
);
vi.mock(
  "./components/BatchPanel",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .BatchPanel,
);
vi.mock(
  "./components/PipelinePage",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .PipelinePage,
);
vi.mock(
  "./components/WorkflowGalleryPage",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .WorkflowGalleryPage,
);
vi.mock(
  "./components/SettingsPage",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .SettingsPage,
);
vi.mock(
  "./components/ResearchActivity",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .ResearchActivity,
);
vi.mock(
  "./components/UpdateBanner",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .UpdateBanner,
);

describe("App launch", () => {
  beforeEach(reset);

  it("rechecks dependencies when reopening a stale setup warning", async () => {
    const defaultInvoke = invoke.getMockImplementation()!;
    let ready = false;
    let checks = 0;
    invoke.mockImplementation(async (command: string, args: unknown) => {
      const result = await defaultInvoke(command, args);
      if (command !== "get_execution_plan") return result;
      checks += 1;
      return {
        ...result,
        readiness: {
          ready,
          deps: [
            {
              name: "ChatGPT (reviews)",
              found: true,
              required: true,
              version: "0.153.4",
              path: "",
              hint: ready
                ? ""
                : "Sign in with a ChatGPT account in Settings → Connections → OpenAI.",
              authenticated: ready,
            },
          ],
        },
      };
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Dismiss" }));
    ready = true;
    await user.click(screen.getByRole("button", { name: "Setup needed" }));
    expect(await screen.findByRole("button", { name: "Close" })).toBeVisible();
    expect(screen.getByText("Subscription")).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Setup needed" }),
    ).not.toBeInTheDocument();
    expect(checks).toBe(2);
  });

  it("does not rebuild the execution plan when per-report agents change", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Choose test paper" }),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Start review" }),
      ).toBeEnabled(),
    );
    await waitFor(() =>
      expect(
        invoke.mock.calls.filter(
          ([command]) => command === "get_execution_plan",
        ),
      ).toHaveLength(1),
    );
    await user.click(
      screen.getByRole("button", { name: "Change report agents" }),
    );

    expect(screen.getByRole("button", { name: "Start review" })).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "Loading workflow…" }),
    ).not.toBeInTheDocument();
    expect(
      invoke.mock.calls.filter(([command]) => command === "get_execution_plan"),
    ).toHaveLength(1);
  });

  it("launches several selected documents as a batch from New review", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Choose two papers" }),
    );
    await user.click(screen.getByRole("button", { name: "Start review" }));
    expect(await screen.findByText(/2 documents · document/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Queue 2 reviews" }));

    await waitFor(() =>
      expect(enqueueBatch).toHaveBeenCalledWith({
        paths: ["/tmp/a.pdf", "/tmp/b.pdf"],
        variables: null,
        extraInputs: null,
        expectedProfileConfigSnapshotId: "config-test",
      }),
    );
    // Each document is its own queued review in Activity; the first opens.
    expect(
      await screen.findByRole("heading", { name: "a.pdf is queued" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: /^Activity/ }));
    expect(screen.getByRole("button", { name: /^b\.pdf/ })).toBeVisible();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "New review" })).toBeEnabled();
    expect(invoke).not.toHaveBeenCalledWith("start_batch", expect.anything());

    await openTools(user);
    await user.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("History workspace")).toBeVisible();
    expect(enqueue).not.toHaveBeenCalled();
  });

  it("defers exact DOCX readiness checks until the user reviews the report", async () => {
    invoke.mockImplementation(
      (command: string, args?: { paperPath?: string | null }) => {
        if (command === "mark_smoke_ready") return Promise.resolve(true);
        if (command === "get_batch_status") return Promise.resolve([]);
        if (command === "get_pipeline_config")
          return Promise.resolve(previewConfig);
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
                  deps: [
                    {
                      name: "PDF extractor",
                      found: false,
                      version: "",
                      path: "",
                      required: true,
                      hint: "Only needed for PDF inputs.",
                    },
                  ],
                },
            stages: [],
          });
        }
        return Promise.reject(new Error(`unexpected command: ${command}`));
      },
    );
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Choose DOCX" }),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Start review" }),
      ).toBeEnabled(),
    );
    expect(
      invoke.mock.calls.filter(
        ([command, args]) =>
          command === "get_execution_plan" &&
          (args as { paperPath?: string } | undefined)?.paperPath ===
            "/tmp/test-paper.docx",
      ),
    ).toHaveLength(0);
    expect(
      screen.queryByRole("button", { name: "Refresh" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    await user.click(screen.getByRole("button", { name: "Start review" }));
    expect(
      await screen.findByRole("dialog", { name: "Ready to start" }),
    ).toBeVisible();
    await user.click(
      within(screen.getByRole("dialog", { name: "Ready to start" })).getByRole(
        "button",
        { name: "Start review" },
      ),
    );

    await waitFor(() =>
      expect(enqueue).toHaveBeenCalledWith(
        expect.objectContaining({
          paperPath: "/tmp/test-paper.docx",
          inputInterpretation: "document",
          expectedProfileSnapshotId: "doc-runtime",
        }),
      ),
    );
    expect(
      invoke.mock.calls.filter(
        ([command, args]) =>
          command === "get_execution_plan" &&
          (args as { paperPath?: string } | undefined)?.paperPath ===
            "/tmp/test-paper.docx",
      ),
    ).toHaveLength(1);
  });

  it("does not rerun workflow preflight after a same-mode workflow save", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Choose DOCX" }),
    );
    await openTools(user);
    await user.click(screen.getByRole("button", { name: "Designer" }));
    await user.click(
      await screen.findByRole("button", {
        name: "Save document workflow",
      }),
    );
    await user.click(
      screen.getByRole("button", {
        name: "Close workflow editor",
      }),
    );

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Start review" }),
      ).toBeEnabled(),
    );
    expect(
      invoke.mock.calls.filter(
        ([command, args]) =>
          command === "get_execution_plan" &&
          (args as { paperPath?: string } | undefined)?.paperPath ===
            "/tmp/test-paper.docx",
      ),
    ).toHaveLength(0);
  });

  it("clears an incompatible primary selection when the workflow input mode changes", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Choose test paper" }),
    );
    await openTools(user);
    await user.click(screen.getByRole("button", { name: "Designer" }));
    await user.click(
      await screen.findByRole("button", {
        name: "Switch to folder workflow",
      }),
    );
    await user.click(
      screen.getByRole("button", {
        name: "Close workflow editor",
      }),
    );

    expect(await screen.findByText("Primary mode: folder")).toBeVisible();
    expect(screen.getByRole("button", { name: "Start review" })).toBeDisabled();
    expect(invoke).toHaveBeenCalledWith("get_run_setup");
    expect(
      invoke.mock.calls.filter(
        ([command, args]) =>
          command === "get_execution_plan" &&
          (args as { paperPath?: string } | undefined)?.paperPath !== null,
      ),
    ).toHaveLength(0);
  });

  it("reloads the new settings snapshot without comparing it to the old fingerprint", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("button", { name: "Start review" });

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(
      await screen.findByRole("button", {
        name: "Save mocked settings",
      }),
    );

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
          deps: [
            {
              name: "Claude CLI",
              found: false,
              version: "",
              path: "",
              required: true,
              hint: "Install or configure model access.",
            },
          ],
        },
        stages: [],
      });
    });

    expect(screen.getByText("Settings workspace")).toBeVisible();
    expect(
      screen.queryByRole("dialog", { name: "Dependencies" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Setup needed" })).toBeVisible();
  });

  it("opens PDF extraction settings from a missing PaddleOCR dependency", async () => {
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
            deps: [
              {
                name: "PaddleOCR-VL Full Parser",
                found: false,
                version: "",
                path: "",
                required: true,
                hint: "Recommended for PDFs: Install from Settings → Reviews → PDF Extraction.",
              },
            ],
          },
          stages: [],
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("link", {
        name: "Settings → Reviews → PDF extraction",
      }),
    );

    expect(
      screen.queryByRole("dialog", { name: "Dependencies" }),
    ).not.toBeInTheDocument();
    expect(await screen.findByText("Settings workspace")).toBeVisible();
    expect(
      screen.getByText("Initial settings section: extraction"),
    ).toBeVisible();
    expect(
      screen.getByText("Settings target: paddleocr-local-engine"),
    ).toBeVisible();
  });

  it("fails closed when fresh profile readiness reports a secondary provider missing", async () => {
    let checks = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_pipeline_config")
        return Promise.resolve(previewConfig);
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
          readiness:
            checks === 1
              ? { deps: [], ready: true }
              : {
                  ready: false,
                  deps: [
                    {
                      name: "Google API",
                      found: false,
                      version: "",
                      path: "",
                      required: true,
                      hint: "Required by the explicit empirical agent.",
                    },
                  ],
                },
          stages: [],
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Choose test paper" }),
    );
    await user.click(screen.getByRole("button", { name: "Start review" }));

    expect(
      await screen.findByRole("dialog", { name: "Dependencies" }),
    ).toBeVisible();
    expect(
      screen.getByText(/Check the required model connections/),
    ).toBeVisible();
    expect(enqueue).not.toHaveBeenCalled();
    expect(checks).toBe(2);
  });

  it("rejects setup when the active profile changes before launch", async () => {
    let plans = 0;
    invoke.mockImplementation((command: string) => {
      if (command === "mark_smoke_ready") return Promise.resolve(true);
      if (command === "get_batch_status") return Promise.resolve([]);
      if (command === "get_pipeline_config")
        return Promise.resolve(previewConfig);
      if (command === "get_run_setup") {
        return Promise.resolve(runSetup("first", "first-config"));
      }
      if (command === "get_execution_plan") {
        plans += 1;
        if (plans > 1)
          return Promise.reject(new Error("active profile changed"));
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

    await user.click(
      await screen.findByRole("button", { name: "Choose test paper" }),
    );
    await user.click(screen.getByRole("button", { name: "Start review" }));

    expect(
      (await screen.findAllByText(/active profile changed/)).length,
    ).toBeGreaterThan(0);
    expect(enqueue).not.toHaveBeenCalled();
  });

  it("keeps the current destination when queue submission finishes after navigation", async () => {
    const user = userEvent.setup();
    let finish!: (run: ReturnType<typeof queuedRun>) => void;
    enqueue.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Choose test paper" }),
    );
    await user.click(screen.getByRole("button", { name: "Start review" }));
    await user.click(
      within(
        await screen.findByRole("dialog", { name: "Ready to start" }),
      ).getByRole("button", { name: "Start review" }),
    );
    await waitFor(() => expect(enqueue).toHaveBeenCalledTimes(1));
    await openTools(user);
    await user.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("History workspace")).toBeVisible();
    await act(async () => finish(queuedRun("late")));
    expect(screen.getByText("History workspace")).toBeVisible();
    expect(router.route).toEqual({ page: "history" });
  });

  it("keeps the chosen input visible after leaving the setup, and starts with it", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Choose test paper" }),
    );
    expect(screen.getByText("Shown input: /tmp/test-paper.pdf")).toBeVisible();

    await openTools(user);
    await user.click(screen.getByRole("button", { name: "Designer" }));
    await user.click(
      await screen.findByRole("button", { name: "Close workflow editor" }),
    );

    expect(
      await screen.findByText("Shown input: /tmp/test-paper.pdf"),
    ).toBeVisible();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Start review" }),
      ).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "Start review" }));
    await user.click(
      within(
        await screen.findByRole("dialog", { name: "Ready to start" }),
      ).getByRole("button", { name: "Start review" }),
    );
    await waitFor(() =>
      expect(enqueue).toHaveBeenCalledWith(
        expect.objectContaining({ paperPath: "/tmp/test-paper.pdf" }),
      ),
    );
  });

  it("reopens the run options with their values when the launch is refused", async () => {
    const defaultInvoke = invoke.getMockImplementation()!;
    invoke.mockImplementation(
      (
        command: string,
        args?: { expectedProfileConfigSnapshotId?: string | null },
      ) => {
        if (command === "get_run_setup")
          return Promise.resolve({
            ...runSetup(),
            variables: [{ key: "code", label: "Code" }],
          });
        if (
          command === "get_execution_plan" &&
          args?.expectedProfileConfigSnapshotId
        )
          return Promise.reject(
            new Error("Code does not match the required format."),
          );
        return defaultInvoke(command, args);
      },
    );
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Choose test paper" }),
    );
    await user.click(screen.getByRole("button", { name: "Start review" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Report options",
    });
    await user.type(
      within(dialog).getByRole("textbox", { name: "Code" }),
      "abc",
    );
    await user.click(within(dialog).getByRole("button", { name: "Continue" }));

    const reopened = await screen.findByRole("dialog", {
      name: "Report options",
    });
    expect(within(reopened).getByRole("alert")).toHaveTextContent(
      "Code does not match the required format.",
    );
    expect(within(reopened).getByRole("textbox", { name: "Code" })).toHaveValue(
      "abc",
    );

    await user.click(within(reopened).getByRole("button", { name: "Cancel" }));
    expect(screen.getByText("The review could not be started.")).toBeVisible();
    expect(
      screen.queryByText("Workflow setup could not be loaded."),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start review" })).toBeEnabled();
    expect(enqueue).not.toHaveBeenCalled();
  });

  it("reopens refused run options from the reloaded setup when the workflow changed", async () => {
    const defaultInvoke = invoke.getMockImplementation()!;
    let setupId = "first-config";
    invoke.mockImplementation(
      (
        command: string,
        args?: { expectedProfileConfigSnapshotId?: string | null },
      ) => {
        if (command === "get_run_setup")
          return Promise.resolve({
            ...runSetup("deep-review", setupId),
            variables: [
              { key: "code", label: "Code" },
              ...(setupId === "second-config"
                ? [{ key: "venue", label: "Venue" }]
                : []),
            ],
          });
        if (
          command === "get_execution_plan" &&
          args?.expectedProfileConfigSnapshotId === "first-config"
        ) {
          // The workflow is saved while the options are being filled in.
          setupId = "second-config";
          return Promise.reject(
            new Error(
              "The active profile or settings changed while run inputs were being collected. Reload the run setup and try again.",
            ),
          );
        }
        return defaultInvoke(command, args);
      },
    );
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Choose test paper" }),
    );
    await user.click(screen.getByRole("button", { name: "Start review" }));
    const dialog = await screen.findByRole("dialog", {
      name: "Report options",
    });
    await user.type(
      within(dialog).getByRole("textbox", { name: "Code" }),
      "abc",
    );
    await user.click(within(dialog).getByRole("button", { name: "Continue" }));

    const reopened = await screen.findByRole("dialog", {
      name: "Report options",
    });
    expect(within(reopened).getByRole("alert")).toHaveTextContent(
      "The review changed while you were entering run options.",
    );
    expect(within(reopened).getByRole("alert")).not.toHaveTextContent(
      "Reload the run setup",
    );
    expect(within(reopened).getByRole("textbox", { name: "Code" })).toHaveValue(
      "abc",
    );
    await user.type(
      within(reopened).getByRole("textbox", { name: "Venue" }),
      "JPE",
    );
    await user.click(
      within(reopened).getByRole("button", { name: "Continue" }),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "get_execution_plan",
        expect.objectContaining({
          expectedProfileConfigSnapshotId: "second-config",
          variables: { code: "abc", venue: "JPE" },
        }),
      ),
    );
    expect(
      within(
        await screen.findByRole("dialog", { name: "Ready to start" }),
      ).getByRole("button", { name: "Start review" }),
    ).toBeVisible();
  });
});
