import { fireEvent, render, screen } from "@testing-library/react";
import { createRef, type ComponentProps } from "react";
import { expect, it, vi } from "vitest";
import WorkspaceConversationView from "./WorkspaceConversationView";

vi.mock("../lib/workbenchClient", () => ({ workbenchClient: {} }));

const onSend = vi.fn();
const onStop = vi.fn();
const view = (
  props: Partial<ComponentProps<typeof WorkspaceConversationView>> = {},
) => (
  <WorkspaceConversationView
    header={null}
    requests={null}
    composerMenu={<button>Attach</button>}
    contextTray={null}
    snapshot={null}
    renderedItems={[]}
    totalItems={0}
    pendingUser={null}
    stream=""
    selectedMessage={null}
    transcriptStart={0}
    transcriptEnd={0}
    transcriptRef={createRef()}
    messageRef={createRef()}
    error={null}
    draft=""
    disabled={false}
    active={false}
    submitting={false}
    contextBusy={false}
    onDraft={vi.fn()}
    onSend={onSend}
    onStop={onStop}
    onLatest={vi.fn()}
    onEarlier={vi.fn()}
    onNewer={vi.fn()}
    onFollow={vi.fn()}
    {...props}
  />
);

it("uses one control for Send and Stop so the composer row never shifts", () => {
  const page = render(view());
  const control = screen.getByRole("button", { name: "Send" });
  expect(control).toBeDisabled();
  page.rerender(view({ draft: "A question" }));
  expect(control).toBeEnabled();
  fireEvent.click(control);
  expect(onSend).toHaveBeenCalledOnce();
  // While a response runs the same element stops it, even with an empty draft.
  page.rerender(view({ active: true, disabled: true }));
  expect(control).toHaveAccessibleName("Stop response");
  expect(control).toBeEnabled();
  fireEvent.click(control);
  expect(onStop).toHaveBeenCalledOnce();
  expect(onSend).toHaveBeenCalledOnce();
  page.rerender(view({ submitting: true, disabled: true, draft: "Sent" }));
  expect(control).toHaveAccessibleName("Sending…");
  expect(control).toBeDisabled();
});

it("states the send shortcut in the field and has no manual resize grip", () => {
  render(view());
  const field = screen.getByRole("textbox", { name: "Message" });
  expect(field).toHaveAttribute("placeholder", "Ask ChatGPT… Enter to send");
  expect(field).toHaveAttribute("rows", "1");
  expect(field).toHaveClass("workspace-composer-input");
});

it("shows where dropped files will go and when they are being added", () => {
  const page = render(view({ dropState: "over" }));
  expect(screen.getByRole("status")).toHaveTextContent(
    "Drop to add these files to the conversation",
  );
  page.rerender(view({ dropState: "importing" }));
  expect(screen.getByRole("status")).toHaveTextContent("Adding files…");
  page.rerender(view());
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});
