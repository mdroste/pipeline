import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import WorkspaceConversationOutline, {
  type ConversationEntry,
} from "./WorkspaceConversationOutline";

it("pages a large outline and searches response bodies beyond the visible page", () => {
  const entries: ConversationEntry[] = Array.from(
    { length: 302 },
    (_, index) => ({
      id: String(index),
      text:
        index === 301
          ? "A distinctive comparative static"
          : `Discussion ${index}`,
      role: index % 2 ? "ChatGPT" : "You",
    }),
  );
  const jump = vi.fn();
  render(
    <WorkspaceConversationOutline
      entries={entries}
      selectedId={null}
      onJump={jump}
      onClose={vi.fn()}
    />,
  );
  const nav = within(
    screen.getByRole("navigation", { name: "Prompts and responses" }),
  );
  expect(nav.getAllByRole("button")).toHaveLength(100);
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  expect(nav.getAllByRole("button")).toHaveLength(100);
  fireEvent.change(screen.getByLabelText("Outline message type"), {
    target: { value: "ChatGPT" },
  });
  fireEvent.change(screen.getByLabelText("Search prompts and responses"), {
    target: { value: "DISTINCTIVE" },
  });
  fireEvent.click(nav.getByRole("button", { name: /302 · Response/ }));
  expect(jump).toHaveBeenCalledWith("301");
  fireEvent.change(screen.getByLabelText("Outline message type"), {
    target: { value: "You" },
  });
  expect(screen.getByText("No matching messages.")).toBeInTheDocument();
});
