import { useState } from "react";
import userEvent from "@testing-library/user-event";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceProjectDialog from "./WorkspaceProjectDialog";
const mocks = vi.hoisted(() => ({ createWorkspace: vi.fn(), registerWorkspaceRoot: vi.fn(), importPaper: vi.fn(), open: vi.fn() }));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
const record = { id: "ws", name: "Trade", root: null, rootIdentity: null, settingsRevision: 1, revision: 1, archivedAt: null, missingRootAt: null, createdAt: "now", updatedAt: "now" };
beforeEach(() => { vi.clearAllMocks(); mocks.createWorkspace.mockResolvedValue({ record }); mocks.registerWorkspaceRoot.mockResolvedValue({ record: { ...record, root: "/papers/trade", revision: 2 } }); mocks.importPaper.mockResolvedValue({ paper: { id: "p" }, revision: null }); });
afterEach(cleanup);

it("creates a project with an optional folder and paper in one step", async () => {
  const onCreated = vi.fn().mockResolvedValue(undefined);
  render(<WorkspaceProjectDialog onClose={vi.fn()} onCreated={onCreated}/>);
  mocks.open.mockResolvedValueOnce("/papers/trade").mockResolvedValueOnce("/papers/trade/draft.pdf");
  fireEvent.click(screen.getByRole("button", { name: "Choose folder…" }));
  await waitFor(() => expect(screen.getByLabelText("Project name")).toHaveValue("trade"));
  fireEvent.click(screen.getByRole("button", { name: "Choose file…" }));
  await screen.findByText("draft.pdf");
  fireEvent.change(screen.getByLabelText("Project name"), { target: { value: "Trade" } });
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  await waitFor(() => expect(onCreated).toHaveBeenCalledWith(expect.objectContaining({ id: "ws", root: "/papers/trade" })));
  expect(mocks.createWorkspace).toHaveBeenCalledWith(expect.objectContaining({ name: "Trade" }));
  expect(mocks.registerWorkspaceRoot).toHaveBeenCalledWith(expect.objectContaining({ workspaceId: "ws", root: "/papers/trade", expectedRevision: 1 }));
  expect(mocks.importPaper).toHaveBeenCalledWith(expect.objectContaining({ workspaceId: "ws", title: "draft", role: "manuscript", path: "/papers/trade/draft.pdf" }));
});
it("does not create a second project when a later step fails and is retried", async () => {
  const onCreated = vi.fn().mockResolvedValue(undefined);
  mocks.importPaper.mockRejectedValueOnce(new Error("No readable text"));
  render(<WorkspaceProjectDialog onClose={vi.fn()} onCreated={onCreated}/>);
  mocks.open.mockResolvedValueOnce("/papers/scan.pdf");
  fireEvent.click(screen.getByRole("button", { name: "Choose file…" }));
  await screen.findByText("scan.pdf");
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("alert")).toHaveTextContent("The project was created, but a later step failed.");
  expect(onCreated).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
  expect(mocks.createWorkspace).toHaveBeenCalledTimes(1);
  expect(mocks.importPaper).toHaveBeenCalledTimes(2);
});
it("requires a name and closes on Escape", () => {
  const onClose = vi.fn();
  render(<WorkspaceProjectDialog onClose={onClose} onCreated={vi.fn()}/>);
  expect(screen.getByRole("button", { name: "Create project" })).toBeDisabled();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(onClose).toHaveBeenCalled();
  expect(mocks.createWorkspace).not.toHaveBeenCalled();
});


it("contains keyboard focus, inerts background content and restores the opener", async () => {
  const user = userEvent.setup();
  function Example() {
    const [open, setOpen] = useState(false);
    return <><button onClick={() => setOpen(true)}>New project opener</button><button>Background action</button>{open && <WorkspaceProjectDialog onClose={() => setOpen(false)} onCreated={vi.fn()}/>}</>;
  }
  render(<Example/>);
  const opener = screen.getByRole("button", { name: "New project opener" });
  await user.click(opener);
  const name = screen.getByLabelText("Project name");
  await waitFor(() => expect(name).toHaveFocus());
  expect(opener).toHaveAttribute("inert");
  await user.tab({ shift: true });
  expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  await user.tab();
  expect(name).toHaveFocus();
  await user.type(name, "Trade");
  await user.tab({ shift: true });
  expect(screen.getByRole("button", { name: "Create project" })).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(opener).toHaveFocus();
  expect(opener).not.toHaveAttribute("inert");
});

it("keeps the busy dialog open and focusable when all controls are disabled", async () => {
  let finish!: (value: unknown) => void;
  mocks.createWorkspace.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const onClose = vi.fn();
  render(<WorkspaceProjectDialog onClose={onClose} onCreated={vi.fn()}/>);
  fireEvent.change(screen.getByLabelText("Project name"), { target: { value: "Trade" } });
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  fireEvent.keyDown(document, { key: "Tab" });
  expect(screen.getByRole("dialog")).toHaveFocus();
  fireEvent.keyDown(document, { key: "Escape" });
  fireEvent.pointerDown(screen.getByRole("dialog"));
  expect(onClose).not.toHaveBeenCalled();
  finish({ record });
  await waitFor(() => expect(screen.getByRole("dialog")).toHaveAttribute("aria-busy", "false"));
});
