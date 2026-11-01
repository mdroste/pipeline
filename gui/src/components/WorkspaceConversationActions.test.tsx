import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkbenchSession, Workspace } from "../lib/workbenchTypes";
import {
  ConversationMenu,
  MoveConversationDialog,
} from "./WorkspaceConversationActions";

const session = { id: "s", title: "Notes" } as WorkbenchSession;
const actions = {
  disabled: false,
  onRename: vi.fn(),
  onGenerateTitle: vi.fn(),
  onMove: vi.fn(),
  onArchive: vi.fn(),
  onDelete: vi.fn(),
};
const openMenu = () =>
  fireEvent.click(
    screen.getByRole("button", { name: "Conversation actions for Notes" }),
  );

describe("ConversationMenu", () => {
  it("offers working files when the conversation has its own folder", () => {
    const onRevealFiles = vi.fn();
    render(
      <ConversationMenu
        session={session}
        {...actions}
        onRevealFiles={onRevealFiles}
      />,
    );
    openMenu();
    fireEvent.click(
      screen.getByRole("menuitem", { name: "Reveal working files" }),
    );
    expect(onRevealFiles).toHaveBeenCalledOnce();
  });

  it("leaves it out for a conversation in a project", () => {
    render(<ConversationMenu session={session} {...actions} />);
    openMenu();
    expect(
      screen.queryByRole("menuitem", { name: "Reveal working files" }),
    ).toBeNull();
  });
});

describe("MoveConversationDialog", () => {
  afterEach(cleanup);
  const workspaces = [{ id: "p", name: "Paper" }] as Workspace[];
  function Host({ busy = false, onClose = vi.fn() }) {
    const [open, setOpen] = useState(false);
    return (
      <>
        <button onClick={() => setOpen(true)}>Move to project…</button>
        <button>Behind the dialog</button>
        {open && (
          <MoveConversationDialog
            session={session}
            workspaces={workspaces}
            busy={busy}
            error={null}
            onMove={vi.fn()}
            onClose={() => {
              onClose();
              setOpen(false);
            }}
          />
        )}
      </>
    );
  }
  const openDialog = async () => {
    fireEvent.click(screen.getByRole("button", { name: "Move to project…" }));
    await waitFor(() =>
      expect(screen.getByRole("dialog")).toContainElement(
        document.activeElement as HTMLElement,
      ),
    );
  };

  it("takes focus, keeps Tab inside, and returns focus on close", async () => {
    const user = userEvent.setup();
    render(<Host />);
    const trigger = screen.getByRole("button", { name: "Move to project…" });
    trigger.focus();
    await openDialog();
    expect(trigger.closest("[inert]")).not.toBeNull();
    for (let press = 0; press < 5; press += 1) {
      await user.tab();
      expect(screen.getByRole("dialog")).toContainElement(
        document.activeElement as HTMLElement,
      );
    }
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(trigger.closest("[inert]")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("stays open on Escape while a move is running", async () => {
    const onClose = vi.fn();
    render(<Host busy onClose={onClose} />);
    await openDialog();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
