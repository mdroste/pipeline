import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import ProjectLocation from "./ProjectLocation";
import { router } from "../lib/router";
const list = vi.hoisted(() => vi.fn());
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { listWorkspaces: list },
}));
beforeEach(() => {
  router.init({ page: "workspace", projectId: "a" });
  router.setGuard(null);
  list.mockReset().mockResolvedValue({
    workspaces: [
      { id: "a", name: "Trade" },
      { id: "b", name: "Wages" },
    ],
  });
});
it("switches from the persistent desk context through the navigation guard", async () => {
  const guard = vi.fn().mockResolvedValue(false);
  router.setGuard(guard);
  const user = userEvent.setup();
  render(<ProjectLocation projectId="a" name="Trade" disabled={false} />);
  await user.click(
    screen.getByRole("button", { name: "Switch project: Trade" }),
  );
  expect(
    await screen.findByRole("menuitemradio", { name: "Trade" }),
  ).toHaveAttribute("aria-checked", "true");
  await user.click(screen.getByRole("menuitemradio", { name: "Wages" }));
  expect(guard).toHaveBeenCalledWith({
    page: "workspace",
    projectId: "b",
    surface: "project",
    destination: "overview",
  });
  expect(router.route).toEqual({ page: "workspace", projectId: "a" });
  expect(
    screen.getByRole("button", { name: "Switch project: Trade" }),
  ).toHaveFocus();
  router.setGuard(null);
});
it("keeps navigation available when loading the project list fails", async () => {
  list.mockRejectedValueOnce(new Error("offline"));
  const user = userEvent.setup();
  render(<ProjectLocation projectId="a" name="Trade" disabled={false} />);
  await user.click(
    screen.getByRole("button", { name: "Switch project: Trade" }),
  );
  await user.click(
    await screen.findByRole("menuitem", { name: "Retry projects" }),
  );
  expect(
    await screen.findByRole("menuitemradio", { name: "Wages" }),
  ).toBeVisible();
  await user.keyboard("{Escape}");
  await waitFor(() =>
    expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
  );
  expect(
    screen.getByRole("button", { name: "Switch project: Trade" }),
  ).toHaveFocus();
});
