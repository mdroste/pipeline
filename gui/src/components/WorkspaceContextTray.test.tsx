import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspaceContextTray, {
  addContextObject,
  addContextObjectsWhenReady,
} from "./WorkspaceContextTray";
import type { ContextSelection, OpenResearchObject } from "../lib/deskClient";
const mocks = vi.hoisted(() => ({
  context: vi.fn(),
  read: vi.fn(),
  saveContext: vi.fn(),
}));
vi.mock("../lib/deskClient", () => ({ deskClient: mocks }));
const object: OpenResearchObject = {
  kind: "paper",
  id: "revision-one",
  revision: "captured-hash",
  start: 20,
  end: 80,
};
const paper = (id: string): OpenResearchObject => ({
  kind: "paper",
  id,
  revision: "hash",
});
const onError = vi.fn();
const tray = (sessionId = "a", disabled = false) => (
  <WorkspaceContextTray
    workspaceId="w"
    sessionId={sessionId}
    disabled={disabled}
    onError={onError}
  />
);
const loaded = async (sessionId = "a") => {
  await waitFor(() =>
    expect(mocks.context).toHaveBeenLastCalledWith(sessionId),
  );
  await act(async () => {});
};
const sources = () =>
  screen.queryByRole("group", { name: "Conversation sources" });
beforeEach(() => {
  vi.clearAllMocks();
  mocks.context.mockResolvedValue({ revision: 0, items: [] });
  mocks.read.mockImplementation(async (_ws, ref) => ({
    title: `Source ${ref.id}`,
  }));
  mocks.saveContext.mockImplementation(async (_id, current, items) => ({
    revision: current.revision + 1,
    items,
  }));
});
it("persists an exact passage and its role without replacing it with the current document", async () => {
  render(tray());
  await loaded();
  // An empty source list takes no space.
  expect(sources()).not.toBeInTheDocument();
  act(() => addContextObject("w", object));
  expect(await screen.findByText("Source revision-one")).toBeVisible();
  expect(mocks.saveContext).toHaveBeenCalledWith(
    "a",
    { revision: 0, items: [] },
    [{ role: "main", object }],
  );
  fireEvent.click(
    screen.getByRole("button", {
      name: "Source revision-one, used as Main paper. Change how it is used",
    }),
  );
  expect(
    screen.getByRole("menuitemradio", { name: "Main paper" }),
  ).toBeChecked();
  // Roles read as a researcher would say them, not as stored identifiers.
  expect(screen.queryByText("referee_report")).not.toBeInTheDocument();
  fireEvent.click(
    screen.getByRole("menuitemradio", { name: "Referee report" }),
  );
  await waitFor(() =>
    expect(mocks.saveContext).toHaveBeenLastCalledWith(
      "a",
      expect.objectContaining({ revision: 1 }),
      [{ role: "referee_report", object }],
    ),
  );
  await screen.findByRole("button", {
    name: /Source revision-one, used as Referee report/,
  });
  fireEvent.click(
    screen.getByLabelText("Remove Source revision-one from context"),
  );
  await waitFor(() => expect(sources()).not.toBeInTheDocument());
  expect(mocks.saveContext).toHaveBeenLastCalledWith(
    "a",
    expect.objectContaining({ revision: 2 }),
    [],
  );
});
it("refuses source changes while a turn is active", async () => {
  render(tray("a", true));
  await loaded();
  act(() => addContextObject("w", object));
  expect(mocks.saveContext).not.toHaveBeenCalled();
  expect(onError).toHaveBeenCalled();
});
it("holds sources that follow an import until the conversation can take them", async () => {
  const view = render(tray("a", true));
  await loaded();
  act(() =>
    addContextObjectsWhenReady("w", [paper("one"), paper("two"), paper("one")]),
  );
  expect(mocks.saveContext).not.toHaveBeenCalled();
  expect(onError).not.toHaveBeenCalled();
  view.rerender(tray("a", false));
  await waitFor(() =>
    expect(mocks.saveContext).toHaveBeenCalledWith(
      "a",
      { revision: 0, items: [] },
      [
        { role: "main", object: paper("one") },
        { role: "supporting", object: paper("two") },
      ],
    ),
  );
  expect(mocks.saveContext).toHaveBeenCalledTimes(1);
  expect(await screen.findByText("Source two")).toBeVisible();
});
it("keeps the row to a few chips and lists the rest on request", async () => {
  mocks.context.mockResolvedValue({
    revision: 4,
    items: ["one", "two", "three", "four", "five"].map((id) => ({
      role: "supporting",
      object: paper(id),
    })),
  });
  render(tray());
  await screen.findByText("Source three");
  expect(screen.queryByText("Source four")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "+2 more" }));
  expect(screen.getByText("Source four")).toBeVisible();
  fireEvent.click(screen.getByLabelText("Remove Source five from context"));
  await waitFor(() =>
    expect(mocks.saveContext).toHaveBeenCalledWith(
      "a",
      expect.objectContaining({ revision: 4 }),
      expect.not.arrayContaining([
        { role: "supporting", object: paper("five") },
      ]),
    ),
  );
  expect(await screen.findByRole("button", { name: "+1 more" })).toBeVisible();
});
it("ignores a completed source save belonging to the conversation just left", async () => {
  let finish!: (value: ContextSelection) => void;
  mocks.saveContext.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(tray());
  await loaded();
  act(() => addContextObject("w", object));
  await waitFor(() => expect(mocks.saveContext).toHaveBeenCalled());
  view.rerender(tray("b"));
  await loaded("b");
  await act(async () =>
    finish({ revision: 1, items: [{ role: "main", object }] }),
  );
  expect(screen.queryByText("Source revision-one")).not.toBeInTheDocument();
  expect(mocks.context).toHaveBeenLastCalledWith("b");
});
it("reloads authoritative selection after a concurrent edit conflict", async () => {
  mocks.saveContext.mockRejectedValue({ message: "Sources changed; refresh" });
  render(tray());
  await loaded();
  mocks.context.mockResolvedValue({
    revision: 7,
    items: [{ role: "source", object }],
  });
  act(() => addContextObject("w", object));
  await screen.findByText("Source revision-one");
  expect(onError).toHaveBeenCalled();
  expect(
    screen.getByRole("button", {
      name: /Source revision-one, used as Source\./,
    }),
  ).toBeInTheDocument();
});
