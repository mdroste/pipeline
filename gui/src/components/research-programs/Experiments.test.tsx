import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import Experiments from "./Experiments";

const mocks = vi.hoisted(() => ({
  call: vi.fn(),
  records: vi.fn(),
  confirm: vi.fn(),
  completed: { run: (() => {}) as () => void },
}));
vi.mock("../../lib/programClient", () => ({
  programClient: { call: mocks.call },
}));
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: { records: mocks.records },
}));
vi.mock("../../lib/studioClient", () => ({ studioClient: {} }));
vi.mock("../DialogService", async (original) => ({
  ...(await original<typeof import("../DialogService")>()),
  confirmDialog: mocks.confirm,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("../research-studio/Jobs", () => ({
  default: ({ onCompleted }: { onCompleted: () => void }) => {
    mocks.completed.run = onCompleted;
    return null;
  },
}));

const grid = {
  record: { id: "grid", title: "Grid", contentHash: "hash" },
  run: { state: "running", reason: null },
  plan: {
    specifications: [],
    totalRuns: 2,
    timeoutBudgetSeconds: 60,
    capturedInputBytes: 0,
  },
  basePlan: { body: {} },
  attempts: [],
};
const choices = {
  objects: [],
  plans: [],
  profiles: [],
  executions: [],
  checkpoints: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.records.mockResolvedValue([{ id: "grid", title: "Grid" }]);
  mocks.call.mockImplementation(async (_w: string, a: { action: string }) =>
    a.action === "choices" ? choices : grid,
  );
});
afterEach(cleanup);

it("cancels a running grid only after the researcher confirms", async () => {
  render(<Experiments workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Grid" }));
  const cancel = await screen.findByRole("button", { name: "Cancel grid" });
  const controls = () =>
    mocks.call.mock.calls.filter(([, a]) => a.action === "experimentControl");

  mocks.confirm.mockResolvedValueOnce(false);
  fireEvent.click(cancel);
  await waitFor(() => expect(mocks.confirm).toHaveBeenCalledOnce());
  await act(async () => {});
  expect(mocks.confirm.mock.calls[0][0]).toContain("can't be resumed");
  expect(controls()).toEqual([]);

  // Pausing can be resumed, so it needs no warning.
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Pause grid" })).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "Pause grid" }));
  await waitFor(() => expect(controls()).toHaveLength(1));
  expect(mocks.confirm).toHaveBeenCalledOnce();

  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Cancel grid" })).toBeEnabled(),
  );
  mocks.confirm.mockResolvedValueOnce(true);
  fireEvent.click(screen.getByRole("button", { name: "Cancel grid" }));
  await waitFor(() =>
    expect(controls().at(-1)?.[1]).toMatchObject({ control: "cancel" }),
  );
});

it("turns a failed reload after a job into text", async () => {
  const onError = vi.fn();
  render(<Experiments workspaceId="w" onOpen={vi.fn()} onError={onError} />);
  await screen.findByRole("button", { name: "Grid" });
  mocks.records.mockRejectedValueOnce({ message: "Busy" });
  act(() => mocks.completed.run());
  await waitFor(() => expect(onError).toHaveBeenCalledWith("Busy"));
});

it("removes a factor from the prepared grid", async () => {
  render(<Experiments workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  await screen.findByRole("button", { name: "Grid" });
  fireEvent.click(screen.getByRole("button", { name: "Add factor" }));
  expect(screen.getByLabelText("Factor 2 name")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Remove factor 1" }));
  expect(screen.queryByLabelText("Factor 2 name")).toBeNull();
  expect(screen.getByLabelText("Factor 1 name")).toHaveValue("");
});

it.each([
  ["running", ["Pause grid", "Cancel grid"], ["Reconcile"]],
  ["attention", ["Reconcile", "Cancel grid"], ["Pause grid"]],
  ["paused", ["Cancel grid"], ["Pause grid", "Reconcile"]],
])("offers only what a %s grid can do", async (state, offered, absent) => {
  mocks.call.mockImplementation(async (_w: string, a: { action: string }) =>
    a.action === "choices" ? choices : { ...grid, run: { ...grid.run, state } },
  );
  render(<Experiments workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Grid" }));
  await screen.findByRole("button", { name: "Cancel grid" });
  for (const name of offered)
    expect(screen.getByRole("button", { name })).toBeInTheDocument();
  for (const name of absent)
    expect(screen.queryByRole("button", { name })).toBeNull();
});
