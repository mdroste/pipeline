import { describe, expect, it } from "vitest";
import {
  closePane,
  closeTabs,
  moveTab,
  remapLayout,
  restoreLayout,
  showIn,
  splitOff,
  type FileLayout,
} from "./fileGroups";

const one = (tabs: string[], active = tabs[0]): FileLayout => ({
  groups: [{ tabs, active: active ? { path: active } : null }],
  focus: 0,
  direction: "horizontal",
});
const tabsOf = (layout: FileLayout) => layout.groups.map((g) => g.tabs);

describe("restoreLayout", () => {
  it("reads the single-pane layout older versions saved", () => {
    const layout = restoreLayout(
      { tabs: ["a", "b", "a"], active: { path: "b" }, split: "c" },
      true,
    );
    expect(tabsOf(layout)).toEqual([["a", "b"], ["c"]]);
    expect(layout.groups[1].active).toEqual({ path: "c" });
  });
  it("merges a saved second pane when splitting is off", () => {
    const layout = restoreLayout(
      {
        groups: [
          { tabs: ["a"], active: { path: "a" } },
          { tabs: ["b"], active: { path: "b" } },
        ],
        focus: 1,
        direction: "vertical",
      },
      false,
    );
    expect(tabsOf(layout)).toEqual([["a", "b"]]);
    expect(layout.focus).toBe(0);
  });
  it("falls back to one empty pane", () => {
    expect(restoreLayout({ groups: "nope" }, true)).toEqual({
      groups: [{ tabs: [], active: null }],
      focus: 0,
      direction: "horizontal",
    });
  });
});

describe("splitting", () => {
  it("moves a dragged tab into a new pane, but keeps the last one", () => {
    const layout = splitOff(one(["a", "b"], "a"), "a", "vertical");
    expect(tabsOf(layout)).toEqual([["b"], ["a"]]);
    expect(layout.groups[0].active).toEqual({ path: "b" });
    expect(layout).toMatchObject({ direction: "vertical", focus: 1 });
    expect(tabsOf(splitOff(one(["a"]), "a", "horizontal"))).toEqual([
      ["a"],
      ["a"],
    ]);
    expect(tabsOf(splitOff(one(["a", "b"]), "a", "horizontal", true))).toEqual([
      ["a", "b"],
      ["a"],
    ]);
  });
  it("moves tabs between panes and closes a pane left empty", () => {
    const split = splitOff(one(["a", "b"], "a"), "b", "horizontal");
    const moved = moveTab(split, "a", 0, 1, 0);
    expect(tabsOf(moved)).toEqual([["a", "b"]]);
    expect(moved.groups[0].active).toEqual({ path: "a" });
    expect(moved.focus).toBe(0);
  });
  it("closes a tab in one pane without touching the other", () => {
    const split = splitOff(one(["a", "b"]), "a", "horizontal", true);
    expect(tabsOf(closeTabs(split, ["a"], 1))).toEqual([["a", "b"]]);
    expect(tabsOf(closeTabs(split, ["a"]))).toEqual([["b"]]);
  });
  it("keeps every tab when a pane closes", () => {
    const split = splitOff(one(["a", "b"], "b"), "a", "horizontal");
    const opened = showIn(split, { path: "c" });
    const closed = closePane(opened, 1);
    expect(tabsOf(closed)).toEqual([["b", "a", "c"]]);
    expect(closed.groups[0].active).toEqual({ path: "b" });
  });
});

it("reorders within a pane and follows renamed files", () => {
  const layout = moveTab(one(["a", "b", "c"]), "c", 0, 0, 0);
  expect(tabsOf(layout)).toEqual([["c", "a", "b"]]);
  expect(moveTab(layout, "c", 0, 0, -1)).toBe(layout);
  const renamed = remapLayout(layout, (path) => (path === "a" ? "z" : path));
  expect(tabsOf(renamed)).toEqual([["c", "z", "b"]]);
});
