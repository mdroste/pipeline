import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import ObjectPane from "./ObjectPane";

const read = vi.hoisted(() => vi.fn());
vi.mock("../../lib/deskClient", async (original) => ({
  ...(await original<typeof import("../../lib/deskClient")>()),
  deskClient: { read },
}));
vi.mock("../ReportViewer", () => ({ default: () => null }));
vi.mock("../WorkspaceContextTray", () => ({ addContextObject: vi.fn() }));

const object = { kind: "source", id: "s", revision: "r" };

beforeEach(() => read.mockReset());
afterEach(cleanup);

it("stops saying it is opening after a failed read, and can try again", async () => {
  const onError = vi.fn();
  read.mockRejectedValueOnce({ message: "Source is unavailable" });
  render(
    <ObjectPane
      workspaceId="w"
      object={object}
      onClose={vi.fn()}
      onError={onError}
    />,
  );
  expect(
    await screen.findByRole("heading", {
      name: "This item could not be opened",
    }),
  ).toBeInTheDocument();
  expect(onError).toHaveBeenCalledWith("Source is unavailable");

  read.mockResolvedValue({ title: "Card (1994)", text: "Body" });
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() =>
    expect(screen.getByRole("heading", { name: "Card (1994)" })).toBeVisible(),
  );
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
});
