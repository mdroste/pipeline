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
const { invoke, enqueue, setWindowTheme } = mocks;
const appearance: AppTestAppearance = {
  systemIsDark: false,
  systemThemeListener: undefined,
};
const { reset } = createAppTestFixture(mocks, appearance);
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

describe("App shell", () => {
  beforeEach(reset);

  it("supports light, dark, and live system appearance preferences", async () => {
    appearance.systemIsDark = true;
    const user = userEvent.setup();
    render(<App />);

    await waitFor(() => {
      expect(document.documentElement).toHaveClass("dark");
      expect(document.documentElement).not.toHaveClass("theme-switching");
      expect(setWindowTheme).toHaveBeenCalledWith(null);
    });

    await user.click(await screen.findByRole("button", { name: "Settings" }));
    await user.click(
      await screen.findByRole("button", { name: "Use light theme" }),
    );
    expect(localStorage.getItem("theme")).toBe("light");
    expect(document.documentElement).not.toHaveClass("dark");
    expect(setWindowTheme).toHaveBeenLastCalledWith("light");

    act(() =>
      appearance.systemThemeListener?.({
        matches: true,
      } as MediaQueryListEvent),
    );
    expect(document.documentElement).not.toHaveClass("dark");

    await user.click(
      await screen.findByRole("button", { name: "Use system theme" }),
    );
    expect(localStorage.getItem("theme")).toBe("system");
    expect(setWindowTheme).toHaveBeenLastCalledWith(null);
    expect(document.documentElement).toHaveClass("dark");

    act(() =>
      appearance.systemThemeListener?.({
        matches: false,
      } as MediaQueryListEvent),
    );
    expect(document.documentElement).not.toHaveClass("dark");
  });

  it("keeps revision reconciliation out of the main run controls", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(
      screen.queryByRole("checkbox", { name: /compare with previous run/i }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Choose test paper" }));
    await user.click(screen.getByRole("button", { name: "Start review" }));
    expect(
      await screen.findByRole("dialog", { name: "Ready to start" }),
    ).toBeVisible();
    expect(enqueue).not.toHaveBeenCalled();
    await user.click(
      within(screen.getByRole("dialog", { name: "Ready to start" })).getByRole(
        "button",
        { name: "Start review" },
      ),
    );
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_execution_plan", {
        variables: null,
        extraInputs: null,
        expectedProfileConfigSnapshotId: "config-test",
        diff: false,
        paperPath: "/tmp/test-paper.pdf",
        inputInterpretation: "document",
      });
      expect(enqueue).toHaveBeenLastCalledWith({
        paperPath: "/tmp/test-paper.pdf",
        inputInterpretation: "document",
        diff: false,
        variables: null,
        extraInputs: null,
        expectedProfileSnapshotId: "snapshot-test",
      });
      expect(enqueue).toHaveBeenCalledTimes(1);
    });
    expect(
      await screen.findByRole("heading", { name: "test-paper.pdf is queued" }),
    ).toBeVisible();
    expect(router.route).toEqual({ page: "main", runKey: "test-paper" });
    await user.click(screen.getByRole("button", { name: /^Activity/ }));
    expect(
      screen.getByRole("button", { name: /^test-paper\.pdf/ }),
    ).toHaveAttribute("aria-current", "page");
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "New review" })).toBeEnabled();
  });

  it("keeps batch selection in New run instead of the left navigation", async () => {
    render(<App />);

    await screen.findByRole("button", { name: "Start review" });
    expect(
      screen.queryByRole("button", { name: "Batch" }),
    ).not.toBeInTheDocument();
  });

  it("opens Gallery from Designer instead of the primary navigation", async () => {
    const user = userEvent.setup();
    render(<App />);

    await screen.findByRole("button", { name: "Start review" });
    expect(
      screen.queryByRole("button", { name: "Gallery" }),
    ).not.toBeInTheDocument();

    await openTools(user);
    await user.click(screen.getByRole("button", { name: "Designer" }));
    await user.click(await screen.findByRole("button", { name: "Gallery" }));

    expect(await screen.findByText("Gallery workspace")).toBeVisible();
    expect(screen.getByRole("button", { name: "Designer" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("shows run setup only in the new-review workspace", async () => {
    const user = userEvent.setup();
    render(<App />);

    expect(screen.getByTestId("run-setup-panel")).toBeVisible();
    await openTools(user);
    expect(screen.getByRole("button", { name: "New review" })).toHaveAttribute(
      "aria-current",
      "page",
    );

    await openTools(user);
    await user.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("History workspace")).toBeVisible();
    expect(screen.queryByTestId("run-setup-panel")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "History" })).toHaveAttribute(
      "aria-current",
      "page",
    );

    await user.click(screen.getByRole("button", { name: "New review" }));
    expect(screen.getByTestId("run-setup-panel")).toBeVisible();
  });

  it("folds the sidebar to icons, keeps focus on the toggle, and remembers the choice", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<App />);

    expect(screen.queryByTestId("collapsed-nav-bar")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(
      screen.getByRole("button", { name: "Expand sidebar" }),
    ).toHaveFocus();
    expect(screen.getByRole("button", { name: "Home" })).toBeVisible();
    expect(screen.getByTestId("run-setup-panel")).toBeVisible();

    unmount();
    render(<App />);
    const expand = screen.getByRole("button", { name: "Expand sidebar" });
    expect(expand).not.toHaveFocus();

    await user.click(expand);
    expect(
      screen.getByRole("button", { name: "Collapse sidebar" }),
    ).toHaveFocus();
    expect(screen.queryByTestId("collapsed-nav-bar")).not.toBeInTheDocument();
  });
});
