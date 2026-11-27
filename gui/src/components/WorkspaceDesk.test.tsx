import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceDesk, { dockZone } from "./WorkspaceDesk";
import { useDeskDock } from "./deskDock";

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
    screen.queryByRole("menuitemradio", { name: "Project + conversation" }),
  ).not.toBeInTheDocument();
  resize(1000);
  expect(message).toBeVisible();
  expect(notes).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Hide conversation" }));
  expect(message).not.toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Show conversation" }));
  expect(screen.getByLabelText("Message")).toBe(message);
  expect(message).toHaveValue("Keep my draft");
});

it("keeps attention and navigation actions reachable from the focused project", () => {
  const stop = vi.fn();
  const { rerender } = render(
    <WorkspaceDesk
      project={<p>Paper</p>}
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
  fireEvent.click(screen.getByRole("button", { name: "Stop response" }));
  expect(stop).toHaveBeenCalledOnce();
  rerender(
    <WorkspaceDesk project={<p>Paper</p>} assistantRequest={1}>
      <textarea aria-label="Message" />
    </WorkspaceDesk>,
  );
  expect(screen.getByLabelText("Message")).toBeVisible();
  expect(screen.getByText("Paper")).not.toBeVisible();
});

it("keeps the chat beside artifacts and restores the project split on collapse", () => {
  const layout = (open: boolean) => (
    <WorkspaceDesk
      project={<textarea aria-label="Project notes" defaultValue="Notes" />}
      artifacts={
        open
          ? { id: "artifacts", content: <p>Preview</p>, onCollapse: vi.fn() }
          : undefined
      }
    >
      <textarea aria-label="Message" defaultValue="Draft" />
    </WorkspaceDesk>
  );
  const view = render(layout(false));
  resize(1000);
  const notes = screen.getByLabelText("Project notes");
  const message = screen.getByLabelText("Message");
  expect(notes).toBeVisible();
  view.rerender(layout(true));
  const pane = screen.getByRole("region", { name: "Artifacts" });
  expect(pane.parentElement).toBe(message.closest(".workspace-desk-panes"));
  expect(message).toBeVisible();
  expect(notes).not.toBeVisible();
  resize(690);
  expect(pane).toBeVisible();
  expect(message).toBeVisible();
  resize(1000);
  view.rerender(layout(false));
  expect(screen.getByLabelText("Project notes")).toBe(notes);
  expect(notes).toBeVisible();
  expect(message).toBeVisible();
});

it("clamps the split to actual space and restores each project's saved width and view", () => {
  localStorage.setItem("pipeline.workspace.assistantWidth.alpha", "560");
  localStorage.setItem("pipeline.workspace.view.beta", "assistant");
  const view = render(
    <WorkspaceDesk key="alpha" workspaceId="alpha" project={<p>Alpha</p>}>
      <p>Chat content</p>
    </WorkspaceDesk>,
  );
  resize(900);
  // The project keeps 400 px; the conversation takes the rest it asked for.
  expect(
    screen.getByText("Chat content").closest(".workspace-desk-assistant"),
  ).toHaveStyle({ width: "500px" });
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
    "440",
  );
});

it("opens a conversation-only project on its chat unless a layout was chosen", () => {
  const desk = (id: string, conversationFirstRequest: number) => (
    <WorkspaceDesk
      key={id}
      workspaceId={id}
      conversationFirstRequest={conversationFirstRequest}
      project={<p>Overview</p>}
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

it("says one request needs attention in the singular", () => {
  render(
    <WorkspaceDesk project={<p>Paper</p>} attentionCount={1}>
      <textarea aria-label="Message" />
    </WorkspaceDesk>,
  );
  expect(screen.getByRole("status")).toHaveTextContent(
    /^1 request needs attention$/,
  );
});

it("names the assistant pane a conversation in its controls", () => {
  render(
    <WorkspaceDesk project={<p>Paper</p>}>
      <textarea aria-label="Message" />
    </WorkspaceDesk>,
  );
  resize(1000);
  expect(
    screen.getByRole("button", { name: "Hide conversation" }),
  ).toHaveTextContent("Conversation");
  fireEvent.click(screen.getByRole("button", { name: "Project layout" }));
  expect(
    screen.getByRole("menuitemradio", { name: "Conversation only" }),
  ).toBeVisible();
  expect(
    screen.getByRole("menuitemradio", { name: "Project + conversation" }),
  ).toBeVisible();
});

it("shows the conversation's messages at the top only while its pane is hidden", () => {
  render(
    <WorkspaceDesk
      project={<div>Project</div>}
      assistantFeedback={<p>A response is still running.</p>}
    >
      <div>Conversation</div>
    </WorkspaceDesk>,
  );
  resize(1000);
  expect(screen.queryByText("A response is still running.")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Hide conversation" }));
  expect(screen.getByText("A response is still running.")).toBeVisible();
  fireEvent.click(screen.getByRole("button", { name: "Show conversation" }));
  expect(screen.queryByText("A response is still running.")).toBeNull();
  // A narrow window that shows only the project hides the conversation too.
  fireEvent.click(screen.getByRole("button", { name: "Hide conversation" }));
  resize(700);
  expect(screen.getByText("A response is still running.")).toBeVisible();
});

it("docks the conversation left, right or at the bottom from the layout menu", () => {
  render(
    <WorkspaceDesk workspaceId="alpha" project={<p>Project</p>}>
      <p>Chat content</p>
    </WorkspaceDesk>,
  );
  resize(1200);
  const panes = () =>
    screen.getByText("Chat content").closest(".workspace-desk-panes");
  const pane = () =>
    screen.getByText("Chat content").closest(".workspace-desk-assistant");
  expect(panes()).toHaveAttribute("data-dock", "right");
  const choose = (name: string) => {
    fireEvent.click(screen.getByRole("button", { name: "Project layout" }));
    fireEvent.click(screen.getByRole("menuitemradio", { name }));
  };
  choose("Left");
  expect(panes()).toHaveAttribute("data-dock", "left");
  expect(localStorage.getItem("pipeline.workspace.assistantDock.alpha")).toBe(
    "left",
  );
  expect(pane()).toHaveStyle({ width: "440px" });
  choose("Bottom");
  expect(panes()).toHaveAttribute("data-dock", "bottom");
  // At the bottom the pane keeps its own height; its width is the desk's.
  expect(pane()).toHaveStyle({ height: "320px" });
  expect(
    screen.getByRole("separator", { name: "Resize conversation" }),
  ).toHaveAttribute("aria-orientation", "horizontal");
  fireEvent.click(screen.getByRole("button", { name: "Project layout" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Reset layout" }));
  expect(panes()).toHaveAttribute("data-dock", "right");
});

it("lets the conversation take any width the project can spare", () => {
  localStorage.setItem("pipeline.workspace.assistantWidth.alpha", "2000");
  render(
    <WorkspaceDesk workspaceId="alpha" project={<p>Project</p>}>
      <p>Chat content</p>
    </WorkspaceDesk>,
  );
  resize(1600);
  expect(
    screen.getByText("Chat content").closest(".workspace-desk-assistant"),
  ).toHaveStyle({ width: "1200px" });
  expect(localStorage.getItem("pipeline.workspace.assistantWidth.alpha")).toBe(
    "2000",
  );
});

function DragTab() {
  const desk = useDeskDock();
  if (!desk) return <p>No dock</p>;
  return (
    <>
      <button onClick={() => desk.move(40, 300)}>Over the left edge</button>
      <button onClick={() => desk.move(500, 590)}>Over the bottom edge</button>
      <button onClick={() => desk.move(500, 200)}>Over the middle</button>
      <button onClick={() => desk.end(true)}>Drop</button>
      <button onClick={() => desk.end(false)}>Cancel</button>
    </>
  );
}

it("moves the conversation where a dragged tab is dropped, previewing the side", () => {
  const view = render(
    <WorkspaceDesk workspaceId="alpha" project={<p>Project</p>}>
      <DragTab />
    </WorkspaceDesk>,
  );
  resize(1200);
  const panes = document.querySelector<HTMLElement>(".workspace-desk-panes")!;
  panes.getBoundingClientRect = () =>
    ({ left: 0, top: 0, width: 1000, height: 600 }) as DOMRect;
  const press = (name: string) =>
    fireEvent.click(screen.getByRole("button", { name }));
  const hint = () => document.querySelector(".workspace-dock-hint");

  press("Over the left edge");
  expect(hint()).toHaveAttribute("data-zone", "left");
  press("Cancel");
  expect(hint()).toBeNull();
  expect(panes).toHaveAttribute("data-dock", "right");

  press("Over the bottom edge");
  expect(hint()).toHaveAttribute("data-zone", "bottom");
  press("Drop");
  expect(panes).toHaveAttribute("data-dock", "bottom");

  // The middle of the desk is not a side; dropping there changes nothing.
  press("Over the middle");
  expect(hint()).toBeNull();
  press("Drop");
  expect(panes).toHaveAttribute("data-dock", "bottom");

  view.rerender(
    <WorkspaceDesk workspaceId="alpha" project={null}>
      <DragTab />
    </WorkspaceDesk>,
  );
  expect(screen.getByText("No dock")).toBeInTheDocument();
});

it("finds the nearest side within reach of an edge", () => {
  const box = { left: 100, top: 50, width: 1000, height: 500 };
  expect(dockZone(box, 150, 300)).toBe("left");
  expect(dockZone(box, 1050, 100)).toBe("right");
  expect(dockZone(box, 600, 520)).toBe("bottom");
  expect(dockZone(box, 600, 200)).toBeNull();
  expect(dockZone(box, 50, 300)).toBeNull();
});
