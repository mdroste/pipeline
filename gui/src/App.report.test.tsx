import { beforeEach, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
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
  views: {} as Record<string, unknown>,
}));
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
    views: mocks.views,
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
vi.mock("./components/ReportWorkspace", () => ({
  default: ({ runId }: { runId: string }) => {
    if (runId === "run-a") throw new Error("The report could not be drawn.");
    return <p>Report {runId}</p>;
  },
}));

const done = (runId: string) => ({
  state: { kind: "done", runId, markdown: "# Report", report: null },
});

beforeEach(() => {
  reset();
  mocks.runQueue.runs = [
    queuedRun("a", "done", { runId: "run-a" }),
    queuedRun("b", "done", { runId: "run-b" }),
  ];
  mocks.views = { a: done("run-a"), b: done("run-b") };
});

it("opens the next finished report after another one crashed", async () => {
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    router.init({ page: "main", runKey: "a" });
    localStorage.setItem("pipeline.ui.route", "#/reviews/run/a");
    render(<App />);
    expect(await screen.findByText("Something went wrong")).toBeVisible();
    await act(async () => {
      await router.navigate({ page: "main", runKey: "b" });
    });
    expect(await screen.findByText("Report run-b")).toBeVisible();
    expect(screen.queryByText("Something went wrong")).not.toBeInTheDocument();
  } finally {
    consoleError.mockRestore();
    localStorage.clear();
  }
});
