import { render, screen, fireEvent } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import ChatList from "./WorkspaceChatList";
import type { WorkbenchSession } from "../lib/workbenchTypes";
it("preserves row actions, archived chats, paging, and approval status inside Chats", () => {
  const restore = vi.fn(),
    load = vi.fn(),
    open = vi.fn(),
    filter = vi.fn();
  const sessions = [
    { id: "1", title: "Main draft" },
    { id: "2", title: "Older chat", archivedAt: "2026-01-01" },
  ] as WorkbenchSession[];
  const props = {
    sessions,
    sessionId: "1",
    onArchived: filter,
    hasOlderSessions: true,
    onLoadOlderSessions: load,
    onSelectSession: open,
    conversationActions: () => ({
      onArchive: restore,
      onRename: vi.fn(),
      onMove: vi.fn(),
      onGenerateTitle: vi.fn(),
      onDelete: vi.fn(),
    }),
    background: { sessionId: "1", waiting: 1 },
  };
  const view = render(<ChatList {...props} />);
  expect(screen.getByLabelText("Needs your approval")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: /^Main draft/ }));
  expect(open).toHaveBeenCalledWith("1");
  fireEvent.change(screen.getByRole("searchbox"), {
    target: { value: "absent" },
  });
  expect(screen.getByRole("status")).toHaveTextContent("No matching chats");
  fireEvent.click(
    screen.getByRole("button", { name: "Show older conversations" }),
  );
  expect(load).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("button", { name: "Archived" }));
  expect(filter).toHaveBeenCalledWith(true);
  view.rerender(<ChatList {...props} showArchived />);
  fireEvent.change(screen.getByRole("searchbox"), {
    target: { value: "older" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Conversation actions for Older chat" }),
  );
  fireEvent.click(screen.getByRole("menuitem", { name: "Restore" }));
  expect(restore).toHaveBeenCalledOnce();
});
