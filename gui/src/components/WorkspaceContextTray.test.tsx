import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import WorkspaceContextTray, { addContextObject } from "./WorkspaceContextTray";
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
const onError = vi.fn();
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
  render(
    <WorkspaceContextTray
      workspaceId="w"
      sessionId="a"
      disabled={false}
      onError={onError}
    />,
  );
  await screen.findByText("Add exact sources from the research desk.");
  expect(screen.getByLabelText("Conversation sources")).not.toBeVisible();
  act(() => addContextObject("w", object));
  await screen.findByText("Source revision-one");
  expect(screen.getByText("Source revision-one")).not.toBeVisible();
  fireEvent.click(screen.getByText(/Sources · 1/, { selector: "summary" }));
  expect(screen.getByLabelText("Role for Source revision-one")).toBeVisible();
  expect(mocks.saveContext).toHaveBeenCalledWith(
    "a",
    { revision: 0, items: [] },
    [{ role: "main", object }],
  );
  fireEvent.change(screen.getByLabelText("Role for Source revision-one"), {
    target: { value: "referee_report" },
  });
  await waitFor(() =>
    expect(mocks.saveContext).toHaveBeenLastCalledWith(
      "a",
      expect.objectContaining({ revision: 1 }),
      [{ role: "referee_report", object }],
    ),
  );
  fireEvent.click(
    screen.getByLabelText("Remove Source revision-one from context"),
  );
  await screen.findByText("Add exact sources from the research desk.");
  expect(mocks.saveContext).toHaveBeenLastCalledWith(
    "a",
    expect.objectContaining({ revision: 2 }),
    [],
  );
});
it("refuses source changes while a turn is active", async () => {
  render(
    <WorkspaceContextTray
      workspaceId="w"
      sessionId="a"
      disabled
      onError={onError}
    />,
  );
  await screen.findByText("Add exact sources from the research desk.");
  act(() => addContextObject("w", object));
  expect(mocks.saveContext).not.toHaveBeenCalled();
  expect(onError).toHaveBeenCalled();
});
it("ignores a completed source save belonging to the conversation just left", async () => {
  let finish!: (value: ContextSelection) => void;
  mocks.saveContext.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(
    <WorkspaceContextTray
      workspaceId="w"
      sessionId="a"
      disabled={false}
      onError={onError}
    />,
  );
  await screen.findByText("Add exact sources from the research desk.");
  act(() => addContextObject("w", object));
  await waitFor(() => expect(mocks.saveContext).toHaveBeenCalled());
  view.rerender(
    <WorkspaceContextTray
      workspaceId="w"
      sessionId="b"
      disabled={false}
      onError={onError}
    />,
  );
  await screen.findByText("Add exact sources from the research desk.");
  await act(async () =>
    finish({ revision: 1, items: [{ role: "main", object }] }),
  );
  expect(screen.queryByText("Source revision-one")).not.toBeInTheDocument();
  expect(mocks.context).toHaveBeenLastCalledWith("b");
});
it("reloads authoritative selection after a concurrent edit conflict", async () => {
  mocks.saveContext.mockRejectedValue({ message: "Sources changed; refresh" });
  render(
    <WorkspaceContextTray
      workspaceId="w"
      sessionId="a"
      disabled={false}
      onError={onError}
    />,
  );
  await screen.findByText("Add exact sources from the research desk.");
  mocks.context.mockResolvedValue({
    revision: 7,
    items: [{ role: "source", object }],
  });
  act(() => addContextObject("w", object));
  await screen.findByText("Source revision-one");
  expect(onError).toHaveBeenCalled();
  expect(screen.getByLabelText("Role for Source revision-one")).toHaveValue(
    "source",
  );
});
