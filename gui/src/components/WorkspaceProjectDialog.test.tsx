import { useState } from "react";
import userEvent from "@testing-library/user-event";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceProjectDialog from "./WorkspaceProjectDialog";
const mocks = vi.hoisted(() => ({
  findProjectRoots: vi.fn(),
  createWorkspace: vi.fn(),
  registerWorkspaceRoot: vi.fn(),
  importPaper: vi.fn(),
  open: vi.fn(),
}));
vi.mock("../lib/projectRoots", () => ({
  findProjectRoots: mocks.findProjectRoots,
}));
vi.mock("../lib/workbench/workspaceClient", () => ({ workspaceClient: mocks }));
vi.mock("../lib/workbench/researchClient", () => ({ researchClient: mocks }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
const record = {
  id: "ws",
  name: "Trade",
  root: "/papers/trade",
  rootIdentity: null,
  settingsRevision: 1,
  revision: 1,
  archivedAt: null,
  missingRootAt: null,
  createdAt: "now",
  updatedAt: "now",
};
beforeEach(() => {
  vi.clearAllMocks();
  sessionStorage.clear();
  mocks.findProjectRoots.mockResolvedValue([]);
  mocks.createWorkspace.mockResolvedValue({ record });
  mocks.registerWorkspaceRoot.mockResolvedValue({
    record: { ...record, root: "/papers/trade", revision: 2 },
  });
  mocks.importPaper.mockResolvedValue({ paper: { id: "p" }, revision: null });
});
afterEach(cleanup);

it("opens an existing repository's detected source without importing or executing it", async () => {
  const onCreated = vi.fn().mockResolvedValue(undefined);
  mocks.findProjectRoots.mockResolvedValue(["paper/main.tex"]);
  render(
    <WorkspaceProjectDialog
      initialFolder="/papers/trade"
      onClose={vi.fn()}
      onCreated={onCreated}
    />,
  );
  expect(screen.getByLabelText("Project name")).toHaveValue("trade");
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
  expect(JSON.parse(sessionStorage.getItem("pipeline.openFile.ws")!)).toEqual({
    path: "paper/main.tex",
  });
  expect(mocks.importPaper).not.toHaveBeenCalled();
});
it("asks which of multiple roots to open and keeps the same created project", async () => {
  mocks.findProjectRoots.mockResolvedValue(["paper.tex", "slides.tex"]);
  const onCreated = vi.fn().mockResolvedValue(undefined);
  render(
    <WorkspaceProjectDialog
      initialFolder="/papers/trade"
      onClose={vi.fn()}
      onCreated={onCreated}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  const choice = await screen.findByLabelText("Starting document");
  expect(onCreated).not.toHaveBeenCalled();
  fireEvent.change(choice, { target: { value: "slides.tex" } });
  fireEvent.click(screen.getByRole("button", { name: "Open project" }));
  await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
  expect(mocks.createWorkspace).toHaveBeenCalledTimes(1);
  expect(JSON.parse(sessionStorage.getItem("pipeline.openFile.ws")!)).toEqual({
    path: "slides.tex",
  });
});

it("creates a project and its required folder atomically, with an optional paper", async () => {
  const onCreated = vi.fn().mockResolvedValue(undefined);
  render(<WorkspaceProjectDialog onClose={vi.fn()} onCreated={onCreated} />);
  mocks.open
    .mockResolvedValueOnce("/papers/trade")
    .mockResolvedValueOnce("/papers/trade/draft.pdf");
  fireEvent.click(screen.getByRole("button", { name: "Choose folder…" }));
  await waitFor(() =>
    expect(screen.getByLabelText("Project name")).toHaveValue("trade"),
  );
  fireEvent.click(screen.getByRole("button", { name: "Choose file…" }));
  await screen.findByText("draft.pdf");
  fireEvent.change(screen.getByLabelText("Project name"), {
    target: { value: "Trade" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  await waitFor(() =>
    expect(onCreated).toHaveBeenCalledWith(
      expect.objectContaining({ id: "ws", root: "/papers/trade" }),
    ),
  );
  expect(mocks.createWorkspace).toHaveBeenCalledWith(
    expect.objectContaining({ name: "Trade", root: "/papers/trade" }),
  );
  expect(mocks.registerWorkspaceRoot).not.toHaveBeenCalled();
  expect(mocks.importPaper).toHaveBeenCalledWith(
    expect.objectContaining({
      workspaceId: "ws",
      title: "draft",
      role: "manuscript",
      path: "/papers/trade/draft.pdf",
    }),
  );
});
it("does not create a second project when a later step fails and is retried", async () => {
  const onCreated = vi.fn().mockResolvedValue(undefined);
  mocks.importPaper.mockRejectedValueOnce(new Error("No readable text"));
  render(
    <WorkspaceProjectDialog
      initialFolder="/papers/trade"
      onClose={vi.fn()}
      onCreated={onCreated}
    />,
  );
  mocks.open.mockResolvedValueOnce("/papers/scan.pdf");
  fireEvent.click(screen.getByRole("button", { name: "Choose file…" }));
  await screen.findByText("scan.pdf");
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("alert")).toHaveTextContent(
    "The project was created, but a later step failed.",
  );
  expect(onCreated).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
  expect(mocks.createWorkspace).toHaveBeenCalledTimes(1);
  expect(mocks.importPaper).toHaveBeenCalledTimes(2);
});
it("requires a name and closes on Escape", () => {
  const onClose = vi.fn();
  render(<WorkspaceProjectDialog onClose={onClose} onCreated={vi.fn()} />);
  expect(screen.getByRole("button", { name: "Create project" })).toBeDisabled();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(onClose).toHaveBeenCalled();
  expect(mocks.createWorkspace).not.toHaveBeenCalled();
});

it("contains keyboard focus, inerts background content and restores the opener", async () => {
  const user = userEvent.setup();
  function Example() {
    const [open, setOpen] = useState(false);
    return (
      <>
        <button onClick={() => setOpen(true)}>New project opener</button>
        <button>Background action</button>
        {open && (
          <WorkspaceProjectDialog
            onClose={() => setOpen(false)}
            onCreated={vi.fn()}
          />
        )}
      </>
    );
  }
  render(<Example />);
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
  mocks.open.mockResolvedValueOnce("/papers/trade");
  await user.click(screen.getByRole("button", { name: "Choose folder…" }));
  await user.click(name);
  await user.tab({ shift: true });
  expect(screen.getByRole("button", { name: "Create project" })).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(opener).toHaveFocus();
  expect(opener).not.toHaveAttribute("inert");
});

it("keeps the busy dialog open and focusable when all controls are disabled", async () => {
  let finish!: (value: unknown) => void;
  mocks.createWorkspace.mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const onClose = vi.fn();
  render(
    <WorkspaceProjectDialog
      initialFolder="/papers/trade"
      onClose={onClose}
      onCreated={vi.fn()}
    />,
  );
  fireEvent.change(screen.getByLabelText("Project name"), {
    target: { value: "Trade" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  fireEvent.keyDown(document, { key: "Tab" });
  expect(screen.getByRole("dialog")).toHaveFocus();
  fireEvent.keyDown(document, { key: "Escape" });
  fireEvent.pointerDown(screen.getByRole("dialog"));
  expect(onClose).not.toHaveBeenCalled();
  finish({ record });
  await waitFor(() =>
    expect(screen.getByRole("dialog")).toHaveAttribute("aria-busy", "false"),
  );
});
it("creates a project with slides and the selected document type", async () => {
  const onCreated = vi.fn().mockResolvedValue(undefined);
  mocks.open.mockResolvedValue("/research/seminar.pdf");
  render(
    <WorkspaceProjectDialog
      initialFolder="/papers/trade"
      onClose={vi.fn()}
      onCreated={onCreated}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Choose file…" }));
  await screen.findByText("seminar.pdf");
  fireEvent.change(screen.getByLabelText("Document type"), {
    target: { value: "slides" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  await waitFor(() => expect(onCreated).toHaveBeenCalled());
  expect(mocks.importPaper).toHaveBeenCalledWith(
    expect.objectContaining({ documentType: "slides", role: "other" }),
  );
});

it("requires a folder even with a name and document, including direct form submission", async () => {
  const onCreated = vi.fn();
  render(<WorkspaceProjectDialog onClose={vi.fn()} onCreated={onCreated} />);
  mocks.open.mockResolvedValueOnce("/papers/scan.pdf");
  fireEvent.click(screen.getByRole("button", { name: "Choose file…" }));
  await screen.findByText("scan.pdf");
  const submit = screen.getByRole("button", { name: "Create project" });
  expect(submit).toBeDisabled();
  fireEvent.submit(submit.closest("form")!);
  expect(mocks.createWorkspace).not.toHaveBeenCalled();
  expect(onCreated).not.toHaveBeenCalled();
});

it("leaves creation available after the native service rejects a folder", async () => {
  mocks.createWorkspace.mockRejectedValueOnce(
    new Error("Folder is unavailable"),
  );
  render(
    <WorkspaceProjectDialog
      initialFolder="/papers/trade"
      onClose={vi.fn()}
      onCreated={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Folder is unavailable",
  );
  expect(screen.queryByText("Open project anyway")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Choose folder…" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Create project" })).toBeEnabled();
});
