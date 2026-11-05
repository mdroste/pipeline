import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import Search from "./Search";

const mocks = vi.hoisted(() => ({
  index: vi.fn(),
  records: vi.fn(),
  search: vi.fn(),
}));
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: mocks,
}));
vi.mock("../WorkspaceContextTray", () => ({ addContextObject: vi.fn() }));

const page = (generation: number) => ({
  hits: [
    {
      title: "Saved finding",
      text: "An exact excerpt",
      object: { kind: "note", id: "n1", revision: "1", start: 0, end: 5 },
      provenance: "reviewed",
      access: "local",
      completeness: "complete",
    },
  ],
  nextCursor: "cursor",
  index: { generation, complete: true },
  version: 1,
});
let generation = 1;

beforeEach(() => {
  vi.clearAllMocks();
  generation = 1;
  // An incomplete index is checked again at once, which keeps this test quick.
  mocks.index.mockImplementation(async () => ({ generation, complete: false }));
  mocks.records.mockResolvedValue([]);
  mocks.search.mockImplementation(async () => page(generation));
});
afterEach(cleanup);

async function searchFor(text: string) {
  fireEvent.change(screen.getByLabelText("Search project"), {
    target: { value: text },
  });
  fireEvent.click(screen.getByRole("button", { name: "Search" }));
  await screen.findByText("Saved finding");
}

it("keeps results on screen when the index moves on, and says so", async () => {
  render(<Search workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  await searchFor("wages");
  expect(screen.getByRole("button", { name: "More results" })).toBeVisible();
  expect(screen.queryByText(/index changed/)).toBeNull();

  generation = 2;
  expect(
    await screen.findByText(
      "The index changed. Search again for current results.",
    ),
  ).toBeInTheDocument();
  expect(screen.getByText("Saved finding")).toBeInTheDocument();
  // The cursor belongs to the old index, so offering it would only fail.
  expect(screen.queryByRole("button", { name: "More results" })).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Search" }));
  await waitFor(() => expect(screen.queryByText(/index changed/)).toBeNull());
  expect(screen.getByRole("button", { name: "More results" })).toBeVisible();
});

it("clears an earlier failure when a search starts", async () => {
  const onActionStart = vi.fn();
  render(
    <Search
      workspaceId="w"
      onOpen={vi.fn()}
      onError={vi.fn()}
      onActionStart={onActionStart}
    />,
  );
  await searchFor("wages");
  expect(onActionStart).toHaveBeenCalledOnce();
});

it("pages with the query and kind that produced the results", async () => {
  render(<Search workspaceId="w" onOpen={vi.fn()} onError={vi.fn()} />);
  await searchFor("wages");
  fireEvent.change(screen.getByLabelText("Search project"), {
    target: { value: "something else" },
  });
  fireEvent.change(screen.getByLabelText("Search object kind"), {
    target: { value: "note" },
  });
  fireEvent.click(screen.getByRole("button", { name: "More results" }));
  await waitFor(() => expect(mocks.search).toHaveBeenCalledTimes(2));
  expect(mocks.search).toHaveBeenLastCalledWith("w", "wages", null, "cursor");
});

it("leaves the index alone while hidden and resumes on return", async () => {
  const view = (active: boolean) => (
    <Search
      workspaceId="w"
      onOpen={vi.fn()}
      onError={onError}
      active={active}
    />
  );
  const onError = vi.fn();
  const page = render(view(false));
  await act(async () => {});
  expect(mocks.index).not.toHaveBeenCalled();

  page.rerender(view(true));
  await waitFor(() => expect(mocks.index).toHaveBeenCalled());
  page.rerender(view(false));
  await act(async () => {});
  const calls = mocks.index.mock.calls.length;
  await act(() => new Promise((done) => setTimeout(done, 250)));
  expect(mocks.index).toHaveBeenCalledTimes(calls);
});

describe("when the index check fails", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("says so, reports it once, and keeps trying", async () => {
    const onError = vi.fn();
    mocks.index.mockRejectedValue({ message: "Index busy" });
    render(<Search workspaceId="w" onOpen={vi.fn()} onError={onError} />);
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(screen.getByText(/could not be checked/)).toBeInTheDocument();
    expect(screen.queryByText(/Building local index/)).toBeNull();

    await act(() => vi.advanceTimersByTimeAsync(2_000));
    await act(() => vi.advanceTimersByTimeAsync(4_000));
    expect(mocks.index).toHaveBeenCalledTimes(3);
    expect(onError).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledWith("Index busy");

    mocks.index.mockResolvedValue({ generation: 1, complete: true });
    await act(() => vi.advanceTimersByTimeAsync(8_000));
    expect(screen.getByText(/Local index complete/)).toBeInTheDocument();
  });
});
