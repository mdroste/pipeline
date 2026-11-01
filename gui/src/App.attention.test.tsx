import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
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
const { invoke, listen } = mocks;
const appearance: AppTestAppearance = {
  systemIsDark: false,
  systemThemeListener: undefined,
};
const { reset } = createAppTestFixture(mocks, appearance);
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
vi.mock(
  "./components/PaperSelector",
  async () =>
    (await import("./test/appShellFixtures")).createAppComponentMocks(mocks)
      .PaperSelector,
);
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

describe("App attention", () => {
  beforeEach(reset);

  it("replays resolved requests received while the attention snapshot is pending", async () => {
    const original = invoke.getMockImplementation()!;
    let finish!: (events: unknown[]) => void;
    invoke.mockImplementation((command: string, ...args: unknown[]) =>
      command === "workbench_codex_pending_requests"
        ? new Promise<unknown[]>((resolve) => {
            finish = resolve;
          })
        : original(command, ...args),
    );
    render(<App />);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("workbench_codex_pending_requests"),
    );
    const onEvent = listen.mock.calls.find(
      ([name]) => name === "workbench:event",
    )![1];
    await act(async () => {
      onEvent({
        payload: { kind: "serverRequestResolved", requestId: "request-1" },
      });
      finish([{ kind: "serverRequest", requestId: "request-1" }]);
    });
    expect(screen.queryByLabelText("needs attention")).not.toBeInTheDocument();
    await act(async () =>
      onEvent({ payload: { kind: "serverRequest", requestId: "request-2" } }),
    );
    expect(screen.getByLabelText("needs attention")).toBeVisible();
  });

  const approval = {
    kind: "serverRequest",
    epoch: 1,
    requestId: 9,
    sessionId: "session-a",
    params: { threadId: "thread-a" },
  };
  /** A response in another conversation waits on an approval while this page is open. */
  function waitingInBackground() {
    const original = invoke.getMockImplementation()!;
    const session = { id: "session-a", workspaceId: null };
    invoke.mockImplementation((command: string, ...args: unknown[]) => {
      if (command === "workbench_codex_pending_requests")
        return Promise.resolve([approval]);
      if (command === "workbench_active_turn")
        return Promise.resolve({
          sessionId: "session-a",
          title: "Tariff paper",
          threadId: "thread-a",
          turnId: "turn-1",
          epoch: 1,
          setup: false,
          runtime: "codex",
          automated: false,
          composer: true,
        });
      if (command === "workbench_session_snapshot")
        return Promise.resolve({ session, workspace: null, sequence: 1 });
      return original(command, ...args);
    });
  }

  it("shows a waiting approval on pages outside conversations until it resolves", async () => {
    waitingInBackground();
    render(<App />);
    expect(
      await screen.findByText(
        "A response in “Tariff paper” is waiting for your approval.",
      ),
    ).toBeVisible();
    const onEvent = listen.mock.calls.find(
      ([name]) => name === "workbench:event",
    )![1];
    await act(async () =>
      onEvent({ payload: { kind: "serverRequestResolved", requestId: 9 } }),
    );
    expect(
      screen.queryByText(
        "A response in “Tariff paper” is waiting for your approval.",
      ),
    ).not.toBeInTheDocument();
  });

  it("opens the waiting conversation from the Activity tray", async () => {
    waitingInBackground();
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: /^Activity/ }));
    fireEvent.click(
      await screen.findByRole("button", {
        name: /A conversation needs your approval/,
      }),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("workbench_session_snapshot", {
        sessionId: "session-a",
      }),
    );
  });

  it("announces a research change as a notice naming the project", async () => {
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command: string, ...args: unknown[]) =>
      command === "workbench_get_workspace"
        ? Promise.resolve({ id: "project-1", name: "Thesis" })
        : original(command, ...args),
    );
    render(<App />);
    await waitFor(() =>
      expect(
        listen.mock.calls.filter(
          ([name]) => name === "workbench-research-attention",
        ).length,
      ).toBe(2),
    );
    await act(async () => {
      for (const [name, handler] of listen.mock.calls)
        if (name === "workbench-research-attention")
          handler({ payload: { workspaceId: "project-1", checkId: "c" } });
    });
    await waitFor(() =>
      expect(mocks.notify).toHaveBeenCalledWith(
        "Something you’re tracking in Thesis has changed.",
        "info",
        expect.objectContaining({ label: "View changes" }),
      ),
    );
    expect(mocks.notify).toHaveBeenCalledOnce();
    expect(
      screen.queryByRole("button", { name: "View changes" }),
    ).not.toBeInTheDocument();
    await act(async () => mocks.notify.mock.calls[0][2].run());
    await waitFor(() =>
      expect(router.route).toEqual({
        page: "workspace",
        projectId: "project-1",
        surface: "project",
        destination: "checks",
      }),
    );
  });

  it("disposes an attention listener that finishes registering after unmount", async () => {
    let finish!: (dispose: () => void) => void;
    listen.mockImplementation((name: string) =>
      name === "workbench:event"
        ? new Promise<() => void>((resolve) => {
            finish = resolve;
          })
        : Promise.resolve(vi.fn()),
    );
    const { unmount } = render(<App />);
    await waitFor(() =>
      expect(listen).toHaveBeenCalledWith(
        "workbench:event",
        expect.any(Function),
      ),
    );
    unmount();
    const dispose = vi.fn();
    await act(async () => finish(dispose));
    expect(dispose).toHaveBeenCalledTimes(1);
  });
});
