import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceDesk from "./WorkspaceDesk";

let resize: (width: number) => void;
beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private callback: ResizeObserverCallback) {}
      observe(element: HTMLElement) {
        if (element.hasAttribute("data-workspace-desk"))
          resize = (width) =>
            act(() =>
              this.callback(
                [{ contentRect: { width } } as ResizeObserverEntry],
                this as unknown as ResizeObserver,
              ),
            );
      }
      disconnect() {}
    },
  );
});
afterEach(() => vi.unstubAllGlobals());

it("adapts to its container while retaining drafts and restoring the requested split", () => {
  render(
    <WorkspaceDesk
      project={<textarea aria-label="Project notes" defaultValue="Notes" />}
      navigationHidden
      onNavigation={vi.fn()}
    >
      <textarea aria-label="Message" defaultValue="Draft" />
    </WorkspaceDesk>,
  );
  resize(1000);
  const notes = screen.getByLabelText("Project notes");
  const message = screen.getByLabelText("Message");
  fireEvent.change(message, { target: { value: "Keep my draft" } });
  act(() => message.focus());
  resize(700);
  expect(message).toBeVisible();
  expect(message).toHaveFocus();
  expect(notes).not.toBeVisible();
  expect(
    screen.queryByRole("menuitemradio", { name: "Project + chat" }),
  ).not.toBeInTheDocument();
  resize(1000);
  expect(message).toBeVisible();
  expect(notes).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Hide chat" }));
  expect(message).not.toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Show chat" }));
  expect(screen.getByLabelText("Message")).toBe(message);
  expect(message).toHaveValue("Keep my draft");
});

it("keeps attention and navigation actions reachable from the focused project", () => {
  const open = vi.fn();
  const stop = vi.fn();
  const { rerender } = render(
    <WorkspaceDesk
      project={<p>Paper</p>}
      navigationHidden
      onNavigation={open}
      attentionCount={2}
      active
      onStop={stop}
    >
      <textarea aria-label="Message" />
    </WorkspaceDesk>,
  );
  resize(700);
  expect(screen.getByRole("status")).toHaveTextContent(
    "2 requests need attention",
  );
  fireEvent.click(screen.getByRole("button", { name: "Browse projects" }));
  expect(open).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("button", { name: "Stop response" }));
  expect(stop).toHaveBeenCalledOnce();
  rerender(
    <WorkspaceDesk
      project={<p>Paper</p>}
      navigationHidden
      onNavigation={open}
      assistantRequest={1}
    >
      <textarea aria-label="Message" />
    </WorkspaceDesk>,
  );
  expect(screen.getByLabelText("Message")).toBeVisible();
  expect(screen.getByText("Paper")).not.toBeVisible();
});

it("clamps the split to actual space and restores each project's saved width and view", () => {
  localStorage.setItem("pipeline.workspace.assistantWidth.alpha", "560");
  localStorage.setItem("pipeline.workspace.view.beta", "assistant");
  const view = render(
    <WorkspaceDesk
      key="alpha"
      workspaceId="alpha"
      project={<p>Alpha</p>}
      navigationHidden={false}
      onNavigation={vi.fn()}
    >
      <p>Chat content</p>
    </WorkspaceDesk>,
  );
  resize(900);
  expect(
    screen.getByText("Chat content").closest(".workspace-desk-assistant"),
  ).toHaveStyle({ width: "420px" });
  resize(1200);
  expect(
    screen.getByText("Chat content").closest(".workspace-desk-assistant"),
  ).toHaveStyle({ width: "560px" });
  view.rerender(
    <WorkspaceDesk
      key="beta"
      workspaceId="beta"
      assistantRequest={2}
      projectRequest={3}
      resetRequest={1}
      project={<p>Beta</p>}
      navigationHidden={false}
      onNavigation={vi.fn()}
    >
      <p>Chat content</p>
    </WorkspaceDesk>,
  );
  resize(1200);
  expect(screen.getByText("Beta")).not.toBeVisible();
  expect(localStorage.getItem("pipeline.workspace.view.beta")).toBe(
    "assistant",
  );
  fireEvent.click(screen.getByRole("button", { name: "Project layout" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Reset layout" }));
  expect(screen.getByText("Beta")).toBeVisible();
  expect(localStorage.getItem("pipeline.workspace.assistantWidth.beta")).toBe(
    "380",
  );
});

it("opens a conversation-only project on its chat unless a layout was chosen", () => {
  const desk = (id: string, conversationFirstRequest: number) => (
    <WorkspaceDesk
      key={id}
      workspaceId={id}
      conversationFirstRequest={conversationFirstRequest}
      project={<p>Overview</p>}
      navigationHidden
      onNavigation={vi.fn()}
    >
      <p>Chat content</p>
    </WorkspaceDesk>
  );
  const view = render(desk("thin", 0));
  resize(1200);
  expect(screen.getByText("Overview")).toBeVisible();
  view.rerender(desk("thin", 1));
  expect(screen.getByText("Overview")).not.toBeVisible();
  expect(screen.getByText("Chat content")).toBeVisible();
  // The default is not saved: a project that later gains a paper opens split.
  expect(localStorage.getItem("pipeline.workspace.view.thin")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Back to project" }));
  expect(localStorage.getItem("pipeline.workspace.view.thin")).toBe("project");

  localStorage.setItem("pipeline.workspace.view.chosen", "split");
  view.rerender(desk("chosen", 1));
  resize(1200);
  view.rerender(desk("chosen", 2));
  expect(screen.getByText("Overview")).toBeVisible();
});
