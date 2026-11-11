import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import AttachProjectPicker from "./AttachProjectPicker";
const client = vi.hoisted(() => ({
  listWorkspaces: vi.fn(),
  createWorkspace: vi.fn(),
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: client,
}));
beforeEach(() => {
  vi.clearAllMocks();
  client.listWorkspaces.mockResolvedValue({
    workspaces: [{ id: "project", name: "My research" }],
  });
});
it("moves the existing chat before continuing with its attachment", async () => {
  const user = userEvent.setup();
  const onMove = vi.fn().mockResolvedValue(true);
  render(
    <AttachProjectPicker
      onMove={onMove}
      onCreated={vi.fn()}
      onBusy={vi.fn()}
    />,
  );
  await user.selectOptions(await screen.findByRole("combobox"), "project");
  await user.click(
    screen.getByRole("button", { name: "Move chat and continue" }),
  );
  expect(onMove).toHaveBeenCalledWith("project");
  expect(client.createWorkspace).not.toHaveBeenCalled();
});
it("keeps the newly created project available when the chat move fails", async () => {
  const user = userEvent.setup();
  const onMove = vi
    .fn()
    .mockResolvedValueOnce(false)
    .mockResolvedValueOnce(true);
  const project = { id: "new", name: "New research" };
  client.createWorkspace.mockResolvedValue({ record: project });
  const onCreated = vi.fn();
  render(
    <AttachProjectPicker
      onMove={onMove}
      onCreated={onCreated}
      onBusy={vi.fn()}
    />,
  );
  await user.type(
    await screen.findByRole("textbox", { name: "New project name" }),
    "New research",
  );
  await user.click(
    screen.getByRole("button", { name: "Create project and continue" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Couldn't move this chat",
  );
  expect(onCreated).toHaveBeenCalledWith(project);
  await user.click(
    screen.getByRole("button", { name: "Move chat and continue" }),
  );
  await waitFor(() => expect(onMove).toHaveBeenCalledTimes(2));
  expect(client.createWorkspace).toHaveBeenCalledTimes(1);
});
