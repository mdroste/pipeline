import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Jobs from "./Jobs";
import type { JobStatus } from "../../lib/studioClient";
import type { ResearchExecution } from "../../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({ jobs: vi.fn(), log: vi.fn() }));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: mocks,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function job(state: string, validation: Record<string, unknown> = {}) {
  return {
    execution: {
      id: "exec-1",
      adapter: "command",
      command: ["python", "run.py"],
      cwd: "/project",
      inputManifest: {},
      outputManifest: {},
      validation,
    } as unknown as ResearchExecution,
    state,
    owner: "detached",
    durationSeconds: 1,
    canReconcile: false,
    resource: "/project",
  } satisfies JobStatus;
}

/** Serves `chunks` in order, one page per read, then empty pages. */
function serveLog(chunks: string[], truncated = false) {
  const ends = chunks.map((_, index) =>
    chunks.slice(0, index + 1).reduce((sum, chunk) => sum + chunk.length, 0),
  );
  mocks.log.mockImplementation(
    async (_ws: string, _id: string, _stream: string, offset: number) => {
      const index = ends.findIndex((end) => end > offset);
      return index < 0
        ? { text: "", nextOffset: offset, truncated }
        : { text: chunks[index], nextOffset: ends[index], truncated };
    },
  );
}

const flush = () => act(() => vi.advanceTimersByTimeAsync(0));
const advance = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

async function openPanel(container: HTMLElement) {
  const details = container.querySelector("details")!;
  details.open = true;
  fireEvent(details, new Event("toggle"));
  await flush();
}

function stubScroll(element: HTMLElement) {
  Object.defineProperty(element, "scrollHeight", {
    configurable: true,
    get: () => 500,
  });
  Object.defineProperty(element, "clientHeight", {
    configurable: true,
    get: () => 100,
  });
  Object.defineProperty(element, "scrollTop", {
    configurable: true,
    writable: true,
    value: 0,
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  mocks.jobs.mockReset();
  mocks.log.mockReset();
});
afterEach(() => {
  vi.useRealTimers();
});

it("follows new output at the bottom and resumes after being hidden", async () => {
  mocks.jobs.mockResolvedValue([job("running")]);
  serveLog(["line 1\n", "line 2\n", "line 3\n"]);
  const props = { workspaceId: "ws", onCompleted: vi.fn() };
  const view = render(<Jobs {...props} />);
  await flush();
  await openPanel(view.container);
  fireEvent.click(screen.getByRole("button", { name: "python run.py" }));
  await flush();
  const log = screen.getByLabelText("Job log");
  expect(log).toHaveTextContent("line 1");
  stubScroll(log);

  await advance(750);
  expect(log).toHaveTextContent("line 2");
  expect(log.scrollTop).toBe(500);

  // Scrolling up stops following.
  log.scrollTop = 100;
  fireEvent.scroll(log);
  view.rerender(<Jobs {...props} visible={false} />);
  await advance(3000);
  expect(mocks.log).toHaveBeenCalledTimes(2);

  view.rerender(<Jobs {...props} visible />);
  await flush();
  expect(log).toHaveTextContent("line 1 line 2 line 3");
  expect(log.scrollTop).toBe(100);
  expect(mocks.log.mock.calls.map((call) => call[3])).toEqual([0, 7, 14]);
});

it("stops reading an ended job and reports its saved limit per stream", async () => {
  mocks.jobs.mockResolvedValue([
    job("completed", { stdoutTruncated: true, stderrTruncated: false }),
  ]);
  serveLog(["done\n"]);
  const view = render(<Jobs workspaceId="ws" onCompleted={vi.fn()} />);
  await flush();
  await openPanel(view.container);
  fireEvent.click(screen.getByRole("button", { name: "python run.py" }));
  await flush();
  await advance(750);
  await advance(3000);
  expect(mocks.log).toHaveBeenCalledTimes(2);
  expect(screen.getByText(/Log limit reached/)).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Errors and warnings" }));
  await flush();
  expect(screen.queryByText(/Log limit reached/)).not.toBeInTheDocument();
});

it("labels a trimmed display without claiming the saved log was cut", async () => {
  mocks.jobs.mockResolvedValue([job("running")]);
  serveLog(["x".repeat(200_001)]);
  const view = render(<Jobs workspaceId="ws" onCompleted={vi.fn()} />);
  await flush();
  await openPanel(view.container);
  fireEvent.click(screen.getByRole("button", { name: "python run.py" }));
  await flush();
  expect(
    screen.getByText("Showing the last 200,000 characters."),
  ).toBeInTheDocument();
  expect(screen.queryByText(/Log limit reached/)).not.toBeInTheDocument();
});

it("polls rarely while collapsed and idle, and pauses while hidden", async () => {
  mocks.jobs.mockResolvedValue([job("completed")]);
  const props = { workspaceId: "ws", onCompleted: vi.fn() };
  const view = render(<Jobs {...props} />);
  await flush();
  expect(mocks.jobs).toHaveBeenCalledTimes(1);
  await advance(1500);
  expect(mocks.jobs).toHaveBeenCalledTimes(1);
  await advance(8500);
  expect(mocks.jobs).toHaveBeenCalledTimes(2);

  await openPanel(view.container);
  expect(mocks.jobs).toHaveBeenCalledTimes(3);
  await advance(1500);
  expect(mocks.jobs).toHaveBeenCalledTimes(4);

  view.rerender(<Jobs {...props} visible={false} />);
  await advance(20_000);
  expect(mocks.jobs).toHaveBeenCalledTimes(4);
});

it("polls every few seconds while collapsed with a job running", async () => {
  mocks.jobs.mockResolvedValue([job("running")]);
  render(<Jobs workspaceId="ws" onCompleted={vi.fn()} />);
  await flush();
  expect(mocks.jobs).toHaveBeenCalledTimes(1);
  await advance(1500);
  expect(mocks.jobs).toHaveBeenCalledTimes(1);
  await advance(3500);
  expect(mocks.jobs).toHaveBeenCalledTimes(2);
  await advance(5000);
  expect(mocks.jobs).toHaveBeenCalledTimes(3);
});

it("refreshes the project when a job ended while hidden", async () => {
  mocks.jobs.mockResolvedValue([job("running")]);
  const onCompleted = vi.fn();
  const view = render(<Jobs workspaceId="ws" onCompleted={onCompleted} />);
  await flush();
  view.rerender(
    <Jobs workspaceId="ws" onCompleted={onCompleted} visible={false} />,
  );
  mocks.jobs.mockResolvedValue([job("completed")]);
  view.rerender(<Jobs workspaceId="ws" onCompleted={onCompleted} visible />);
  await flush();
  expect(onCompleted).toHaveBeenCalledTimes(1);
});
