import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import FileWorkspace from "./FileWorkspace";
import type { FilePreview } from "../../lib/fileWorkspaceClient";

vi.mock("./SourceEditor", () => ({
  default: ({ value }: { value: string }) => <pre>{value}</pre>,
}));

class TestPointerEvent extends MouseEvent {
  pointerId: number;
  isPrimary: boolean;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
    this.isPrimary = init.isPrimary ?? true;
  }
}
beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal("PointerEvent", TestPointerEvent);
  // Three tabs, each 100 pixels wide, side by side from the strip's left edge.
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      if (this.classList.contains("file-tab")) {
        const index = [...this.parentElement!.children].indexOf(this);
        return new DOMRect(index * 100, 0, 100, 30);
      }
      // The editor below the tabs.
      if (this.classList.contains("file-group-body"))
        return new DOMRect(0, 40, 600, 400);
      return new DOMRect(0, 0, 300, 30);
    },
  );
});
afterEach(async () => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  // A drop ignores the click that follows it until the next task.
  await new Promise((resolve) => setTimeout(resolve));
});

function open(tabs: string[]) {
  localStorage.setItem(
    "pipeline.files.layout.tabs",
    JSON.stringify({ tabs, active: { path: tabs[0] }, split: null }),
  );
  const read = async (path: string): Promise<FilePreview> => ({
    path,
    hash: path,
    text: `text of ${path}`,
    bytes: 10,
    base64: null,
    mime: "text/plain",
    editable: false,
    truncated: false,
    externalPath: null,
  });
  render(<FileWorkspace adapter={{ id: "tabs", read }} paths={tabs} />);
}
const order = () =>
  screen.getAllByRole("tab").map((tab) => tab.textContent?.trim());
const saved = () =>
  JSON.parse(localStorage.getItem("pipeline.files.layout.tabs") ?? "{}")
    .groups[0].tabs;

it("drags a tab into a new place without opening it", async () => {
  open(["a.do", "b.do", "c.do"]);
  await screen.findByText("text of a.do");
  const dragged = screen.getByRole("tab", { name: "c.do" });
  fireEvent.pointerDown(dragged, { clientX: 250 });
  fireEvent.pointerMove(document, { clientX: 150 });
  fireEvent.pointerMove(document, { clientX: 40 });
  fireEvent.pointerUp(document, { clientX: 40 });
  // The release lands on the tab, which must not switch files.
  fireEvent.click(dragged);
  expect(order()).toEqual(["c.do", "a.do", "b.do"]);
  expect(saved()).toEqual(["c.do", "a.do", "b.do"]);
  expect(screen.getByRole("tab", { name: "a.do" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(dragged.parentElement!.style.transform).toBe("");
});

it("moves a tab only once it covers half of its neighbour", () => {
  open(["a.do", "b.do", "c.do"]);
  fireEvent.pointerDown(screen.getByRole("tab", { name: "a.do" }), {
    clientX: 50,
  });
  fireEvent.pointerMove(document, { clientX: 90 });
  fireEvent.pointerUp(document, { clientX: 90 });
  expect(order()).toEqual(["a.do", "b.do", "c.do"]);
  fireEvent.pointerDown(screen.getByRole("tab", { name: "a.do" }), {
    clientX: 50,
  });
  fireEvent.pointerMove(document, { clientX: 110 });
  fireEvent.pointerUp(document, { clientX: 110 });
  expect(order()).toEqual(["b.do", "a.do", "c.do"]);
});

it("keeps a short press as a click and cancels a drag on Escape", async () => {
  open(["a.do", "b.do", "c.do"]);
  const b = screen.getByRole("tab", { name: "b.do" });
  fireEvent.pointerDown(b, { clientX: 150 });
  fireEvent.pointerMove(document, { clientX: 152 });
  fireEvent.pointerUp(document, { clientX: 152 });
  fireEvent.click(b);
  await waitFor(() => expect(b).toHaveAttribute("aria-selected", "true"));
  fireEvent.pointerDown(b, { clientX: 150 });
  fireEvent.pointerMove(document, { clientX: 290 });
  fireEvent.keyDown(document, { key: "Escape" });
  fireEvent.pointerUp(document, { clientX: 290 });
  expect(order()).toEqual(["a.do", "b.do", "c.do"]);
  expect(b.parentElement!.style.transform).toBe("");
});

it("moves the focused tab with Ctrl+Shift+Page Up and Page Down", () => {
  open(["a.do", "b.do", "c.do"]);
  const a = screen.getByRole("tab", { name: "a.do" });
  fireEvent.keyDown(a, { key: "PageDown", ctrlKey: true, shiftKey: true });
  expect(order()).toEqual(["b.do", "a.do", "c.do"]);
  fireEvent.keyDown(a, { key: "PageDown", ctrlKey: true, shiftKey: true });
  fireEvent.keyDown(a, { key: "PageDown", ctrlKey: true, shiftKey: true });
  expect(order()).toEqual(["b.do", "c.do", "a.do"]);
  fireEvent.keyDown(a, { key: "PageUp", ctrlKey: true, shiftKey: true });
  expect(order()).toEqual(["b.do", "a.do", "c.do"]);
});

const frame = () =>
  act(() => new Promise((resolve) => requestAnimationFrame(resolve)));
/** Points `document.elementFromPoint` at whatever `pick` returns. */
const under = (pick: () => Element | null) => {
  document.elementFromPoint = () => pick();
};
const pane = (index: number) =>
  document.querySelector(`.file-group-body[data-group="${index}"]`);
const strip = (index: number) =>
  document.querySelector(`.file-tabs[data-group="${index}"]`);
const panes = () =>
  screen
    .getAllByRole("tablist")
    .map((list) =>
      [...list.querySelectorAll('[role="tab"]')].map((tab) =>
        tab.textContent?.trim(),
      ),
    );

it("splits the editor when a tab is dropped on its right half, with tabs in each pane", async () => {
  open(["a.do", "b.do", "c.do"]);
  await screen.findByText("text of a.do");
  under(() => pane(0));
  fireEvent.pointerDown(screen.getByRole("tab", { name: "c.do" }), {
    clientX: 250,
    clientY: 15,
  });
  fireEvent.pointerMove(document, { clientX: 550, clientY: 200 });
  fireEvent.pointerUp(document, { clientX: 550, clientY: 200 });
  expect(
    screen.getByRole("tablist", { name: "Open files, first pane" }),
  ).toBeInTheDocument();
  expect(panes()).toEqual([["a.do", "b.do"], ["c.do"]]);
  expect(await screen.findByText("text of c.do")).toBeInTheDocument();
  expect(screen.getByText("text of a.do")).toBeInTheDocument();
  expect(saved()).toEqual(["a.do", "b.do"]);

  // Dragging the second pane's only tab back closes that pane.
  await new Promise((resolve) => setTimeout(resolve));
  under(() => strip(0));
  fireEvent.pointerDown(screen.getByRole("tab", { name: "c.do" }), {
    clientX: 50,
    clientY: 15,
  });
  fireEvent.pointerMove(document, { clientX: 150, clientY: 200 });
  fireEvent.pointerUp(document, { clientX: 150, clientY: 200 });
  expect(panes()).toEqual([["a.do", "c.do", "b.do"]]);
  expect(
    screen.getByRole("tablist", { name: "Open files" }),
  ).toBeInTheDocument();
});

it("splits below when the tab lands nearer the bottom, and leaves the top-left alone", async () => {
  open(["a.do", "b.do"]);
  await screen.findByText("text of a.do");
  under(() => pane(0));
  const b = screen.getByRole("tab", { name: "b.do" });
  fireEvent.pointerDown(b, { clientX: 150, clientY: 15 });
  fireEvent.pointerMove(document, { clientX: 100, clientY: 120 });
  await frame();
  expect(document.querySelector(".file-drop-hint")).toBeNull();
  fireEvent.pointerUp(document, { clientX: 100, clientY: 120 });
  expect(panes()).toEqual([["a.do", "b.do"]]);
  await new Promise((resolve) => setTimeout(resolve));
  fireEvent.pointerDown(b, { clientX: 150, clientY: 15 });
  fireEvent.pointerMove(document, { clientX: 200, clientY: 420 });
  await frame();
  expect(document.querySelector(".file-drop-hint")).toHaveAttribute(
    "data-zone",
    "bottom",
  );
  fireEvent.pointerUp(document, { clientX: 200, clientY: 420 });
  expect(panes()).toEqual([["a.do"], ["b.do"]]);
  expect(document.querySelector(".split-view-vertical")).not.toBeNull();
  expect(document.querySelector(".file-drop-hint")).toBeNull();
});

it("opens files from the list into the pane last used, and closing a pane keeps its tabs", async () => {
  localStorage.setItem(
    "pipeline.files.layout.tabs",
    JSON.stringify({
      groups: [
        { tabs: ["a.do"], active: { path: "a.do" } },
        { tabs: ["b.do"], active: { path: "b.do" } },
      ],
      focus: 1,
      direction: "horizontal",
    }),
  );
  const read = async (path: string): Promise<FilePreview> => ({
    path,
    hash: path,
    text: `text of ${path}`,
    bytes: 10,
    base64: null,
    mime: "text/plain",
    editable: false,
    truncated: false,
    externalPath: null,
  });
  render(
    <FileWorkspace
      adapter={{ id: "tabs", read }}
      paths={["a.do", "b.do", "c.do"]}
      initial={{ path: "c.do" }}
    />,
  );
  await screen.findByText("text of c.do");
  expect(panes()).toEqual([["a.do"], ["b.do", "c.do"]]);
  const [, closeSecond] = screen.getAllByRole("button", {
    name: "Close pane",
  });
  fireEvent.click(closeSecond);
  expect(panes()).toEqual([["a.do", "b.do", "c.do"]]);
});
