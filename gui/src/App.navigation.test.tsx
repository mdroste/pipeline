import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
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
const { saveCollection, invoke, onCloseRequested, confirmDialog } = mocks;
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

describe("App navigation", () => {
  beforeEach(reset);

  it("opens on the project-centered Home and starts a review from an outcome card", async () => {
    const user = userEvent.setup();
    localStorage.removeItem("pipeline.ui.page");
    render(<App />);

    expect(
      await screen.findByRole("heading", {
        name: "What would you like to do?",
      }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "Reviews" })).toBeVisible();
    // A first launch (no earlier interface state) also offers guided setup.
    await screen.findByRole("dialog", { name: "Get started with Pipeline" });
    await user.click(screen.getByRole("button", { name: "Skip for now" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Review a paper/ }));
    expect(await screen.findByTestId("run-setup-panel")).toBeVisible();
  });

  it("says so when a listed conversation can't be opened", async () => {
    const user = userEvent.setup();
    localStorage.setItem("pipeline.ui.page", "home");
    localStorage.setItem(
      "pipeline.ui.navRailSections",
      JSON.stringify({ conversations: true }),
    );
    const unhandled = invoke.getMockImplementation()!;
    invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === "workbench_list_workspaces")
        return Promise.resolve({ workspaces: [] });
      if (command === "workbench_list_sessions")
        return Promise.resolve({
          sessions: [{ id: "gone", title: "Deleted", updatedAt: "now" }],
        });
      if (command === "workbench_session_snapshot")
        return Promise.reject(new Error("Conversation not found"));
      return unhandled(command, args);
    });
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Deleted" }));

    await waitFor(() =>
      expect(mocks.notify).toHaveBeenCalledWith(
        expect.stringContaining("Conversation not found"),
      ),
    );
  });

  it("leaves Search closed behind a confirmation", async () => {
    localStorage.setItem("pipeline.ui.page", "home");
    render(<App />);
    await screen.findByRole("button", { name: "Home" });
    const search = () => screen.queryByRole("dialog", { name: "Search" });
    const confirmation = document.createElement("dialog");
    confirmation.setAttribute("open", "");
    document.body.append(confirmation);
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    await act(async () => {});
    expect(search()).toBeNull();

    confirmation.remove();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    await waitFor(() => expect(search()).not.toBeNull());
  });

  it("creates a conversation from New conversation on Home", async () => {
    const user = userEvent.setup();
    localStorage.setItem("pipeline.ui.page", "home");
    const unhandled = invoke.getMockImplementation()!;
    invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === "workbench_create_session")
        return Promise.resolve({ record: { id: "created" } });
      if (command === "workbench_session_snapshot")
        return Promise.resolve({
          session: { id: "created", workspaceId: null },
        });
      return unhandled(command, args);
    });
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: /Ask questions or get help/ }),
    );

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "workbench_create_session",
        expect.objectContaining({
          request: expect.objectContaining({ workspaceId: null }),
        }),
      ),
    );
    await waitFor(() =>
      expect(router.route).toMatchObject({
        page: "workspace",
        sessionId: "created",
      }),
    );
  });

  it("reopens guided setup and the example Review from Home", async () => {
    const user = userEvent.setup();
    localStorage.setItem("pipeline.ui.page", "home");
    render(<App />);
    await screen.findByRole("heading", { name: "What would you like to do?" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Set up Pipeline" }));
    await screen.findByRole("dialog", { name: "Get started with Pipeline" });
    await user.keyboard("{Escape}");
    await user.click(
      screen.getByRole("button", { name: "See an example review" }),
    );
    expect(
      await screen.findByRole("dialog", {
        name: "Example review",
      }),
    ).toBeVisible();
    expect(invoke).not.toHaveBeenCalledWith(
      "save_annotations",
      expect.anything(),
    );
  });

  it("opens Projects as an index even when a project was previously selected", async () => {
    const original = invoke.getMockImplementation()!;
    invoke.mockImplementation((command: string, args: unknown) =>
      command === "workbench_project_index"
        ? Promise.resolve([])
        : original(command, args),
    );
    localStorage.setItem("pipeline.workspace.workspaceId", "previous-project");
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Projects" }));
    expect(
      await screen.findByRole("button", { name: "Create your first project" }),
    ).toBeVisible();
    expect(invoke).not.toHaveBeenCalledWith(
      "workbench_project_home",
      expect.anything(),
    );
    expect(localStorage.getItem("pipeline.ui.page")).toBe("project-index");
  });

  it("shows the first-run data notice once and remembers acknowledgement", async () => {
    const user = userEvent.setup();
    const firstRender = render(<App />);

    expect(
      screen.getByRole("note", { name: "Data and privacy" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Got it" }));
    expect(
      screen.queryByRole("note", { name: "Data and privacy" }),
    ).not.toBeInTheDocument();
    await screen.findByRole("button", { name: "Start review" });

    firstRender.unmount();
    render(<App />);
    expect(
      screen.queryByRole("note", { name: "Data and privacy" }),
    ).not.toBeInTheDocument();
    await screen.findByRole("heading", { name: "What would you like to do?" });
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
    await user.click(
      await screen.findByRole("button", { name: "Install in Settings" }),
    );

    expect(await screen.findByText("Settings workspace")).toBeVisible();
    expect(
      screen.getByText("Initial settings section: extraction"),
    ).toBeVisible();
    expect(
      screen.getByText("Settings target: paddleocr-local-engine"),
    ).toBeVisible();
  });

  it("blocks in-app navigation away from unsaved settings", async () => {
    const user = userEvent.setup();
    confirmDialog.mockResolvedValueOnce(false);
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(
      await screen.findByRole("button", { name: "Make settings dirty" }),
    );
    await openTools(user);
    await user.click(screen.getByRole("button", { name: "History" }));

    expect(confirmDialog).toHaveBeenCalledWith(
      "You have unsaved settings changes. Leave and discard them?",
      expect.anything(),
    );
    expect(screen.getByText("Settings workspace")).toBeVisible();
    expect(screen.queryByText("History workspace")).not.toBeInTheDocument();
  });

  it("blocks activity destinations without mutating them when settings are dirty", async () => {
    const user = userEvent.setup();
    confirmDialog.mockResolvedValueOnce(false);
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(
      await screen.findByRole("button", { name: "Make settings dirty" }),
    );
    // Activity is a destination now; the leave guard runs before it opens.
    await user.click(screen.getByRole("button", { name: "Activity" }));
    await user.click(screen.getByRole("button", { name: "See all activity" }));

    expect(confirmDialog).toHaveBeenCalledWith(
      "You have unsaved settings changes. Leave and discard them?",
      expect.anything(),
    );
    expect(screen.getByText("Settings workspace")).toBeVisible();
    expect(localStorage.getItem("pipeline.workspace.workspaceId")).toBeNull();
  });

  it("blocks native window close while workflow changes are unsaved", async () => {
    const user = userEvent.setup();
    confirmDialog.mockResolvedValueOnce(false);
    render(<App />);

    await waitFor(() => expect(onCloseRequested).toHaveBeenCalledTimes(1));
    await openTools(user);
    await user.click(screen.getByRole("button", { name: "Designer" }));
    await user.click(
      await screen.findByRole("button", { name: "Make workflow dirty" }),
    );

    const event = { preventDefault: vi.fn() };
    const closeHandler = onCloseRequested.mock.calls[0][0] as (closeEvent: {
      preventDefault: () => void;
    }) => void;
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
      "Pipeline cannot warn you about unsaved changes when you quit",
    );
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeVisible();
  });
  it("keeps collection edits on the page when a save fails, then allows a successful retry", async () => {
    const user = userEvent.setup();
    render(<App />);
    // Collections is no longer a rail destination; reach it by route (the
    // in-app paths are History → Collections and a project's Reviews view).
    await act(async () => {
      await router.navigate({ page: "projects" });
    });
    await user.click(
      await screen.findByRole("button", { name: "Edit collection" }),
    );
    saveCollection.mockResolvedValue(false);
    await openTools(user);
    await user.click(screen.getByRole("button", { name: "History" }));
    expect(
      screen.getByRole("button", { name: "Edit collection" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("History workspace")).not.toBeInTheDocument();
    const event = { preventDefault: vi.fn() };
    await act(async () => onCloseRequested.mock.calls[0][0](event));
    expect(event.preventDefault).toHaveBeenCalled();
    saveCollection.mockResolvedValue(true);
    await openTools(user);
    await user.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("History workspace")).toBeInTheDocument();
  });

  it("honors returning to the collection while navigation is waiting for its save", async () => {
    const user = userEvent.setup();
    render(<App />);
    await act(async () => {
      await router.navigate({ page: "projects" });
    });
    await screen.findByRole("button", { name: "Edit collection" });
    let resolve!: (saved: boolean) => void;
    saveCollection.mockImplementation(
      () =>
        new Promise<boolean>((done) => {
          resolve = done;
        }),
    );
    await openTools(user);
    await user.click(screen.getByRole("button", { name: "History" }));
    const returning = router.navigate({ page: "projects" });
    await act(async () => resolve(true));
    await act(async () => {
      await returning;
    });
    expect(
      screen.getByRole("button", { name: "Edit collection" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("History workspace")).not.toBeInTheDocument();
  });
});
