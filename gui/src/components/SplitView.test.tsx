import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import SplitView from "./SplitView";
let width = 1000, height = 600;
const observers = new Set<ResizeObserverCallback>();
beforeEach(() => {
  localStorage.clear(); width = 1000; height = 600; observers.clear();
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function(this: HTMLElement) { return { width: this.classList.contains("split-view-body") ? width : 0, height: this.classList.contains("split-view-body") ? height : 0 } as DOMRect; });
  vi.stubGlobal("ResizeObserver", class { constructor(callback: ResizeObserverCallback) { observers.add(callback); } observe() {} disconnect() {} });
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
function view() { return <SplitView first={<textarea aria-label="Draft" defaultValue="Saved text"/>} second={<button>Preview content</button>} firstLabel="Source" secondLabel="Preview" storageKey="test.split"/>; }
it("persists a keyboard resize and arrangement, then restores them", () => {
  const { unmount } = render(view());
  fireEvent.keyDown(screen.getByRole("separator"), {key:"ArrowRight", shiftKey:true});
  const saved = JSON.parse(localStorage.getItem("test.split")!);
  expect(saved.ratio).toBeGreaterThan(.5);
  fireEvent.change(screen.getByRole("combobox"), {target:{value:"vertical"}});
  expect(screen.getByRole("separator")).toHaveAttribute("aria-orientation", "horizontal");
  unmount(); render(view());
  expect(screen.getByRole("combobox")).toHaveValue("vertical");
  expect(Number(screen.getByRole("separator").getAttribute("aria-valuenow"))).toBeGreaterThan((height - 12) / 2);
});
it("keeps both panes mounted and preserves the focused pane and draft when space runs out", () => {
  render(view());
  const input = screen.getByRole("textbox"); fireEvent.change(input, {target:{value:"Unsaved"}});
  fireEvent.focus(screen.getByRole("button", {name:"Preview content"}));
  width = 400; height = 250;
  act(() => observers.forEach(callback => callback([], {} as ResizeObserver)));
  expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  expect(screen.getByRole("button", {name:"Preview content"})).toBeVisible();
  expect(input).toHaveValue("Unsaved"); expect(input).not.toBeVisible();
  fireEvent.click(screen.getByRole("button", {name:"First pane"}));
  expect(input).toBeVisible();
  width = 1000; height = 600;
  act(() => observers.forEach(callback => callback([], {} as ResizeObserver)));
  fireEvent.click(screen.getByRole("button", {name:"Both panes"}));
  expect(screen.getByRole("separator")).toBeVisible();
  expect(screen.getByRole("textbox")).toBe(input);
});
it("recovers from malformed saved preferences", () => {
  localStorage.setItem("test.split", "{broken"); render(view());
  expect(screen.getByRole("combobox")).toHaveValue("auto");
  expect(screen.getByRole("separator")).toHaveAttribute("aria-valuenow", "494");
});
