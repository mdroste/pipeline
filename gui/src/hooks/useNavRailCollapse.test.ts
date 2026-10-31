import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import useNavRailCollapse from "./useNavRailCollapse";

let narrow = false;
const listeners = new Set<() => void>();
const resize = (next: boolean) => {
  narrow = next;
  act(() => listeners.forEach((listener) => listener()));
};

beforeEach(() => {
  localStorage.clear();
  narrow = false;
  listeners.clear();
  vi.stubGlobal("matchMedia", () => ({
    get matches() {
      return narrow;
    },
    addEventListener: (_: string, listener: () => void) =>
      listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) =>
      listeners.delete(listener),
  }));
});

it("follows the window width until the researcher chooses", () => {
  const { result } = renderHook(() => useNavRailCollapse());
  expect(result.current.collapsed).toBe(false);
  resize(true);
  expect(result.current.collapsed).toBe(true);
  resize(false);
  expect(result.current.collapsed).toBe(false);
});

it("keeps a choice against the window width, and undoing it follows again", () => {
  const { result } = renderHook(() => useNavRailCollapse());
  resize(true);
  act(() => result.current.toggle());
  expect(result.current.collapsed).toBe(false);
  expect(localStorage.getItem("pipeline.ui.navRailMode")).toBe("expanded");

  act(() => result.current.toggle());
  expect(result.current.collapsed).toBe(true);
  expect(localStorage.getItem("pipeline.ui.navRailMode")).toBe("auto");
  resize(false);
  expect(result.current.collapsed).toBe(false);
});

it("treats a sidebar hidden by an older version as folded", () => {
  localStorage.setItem("pipeline.ui.navRailCollapsed", "true");
  const { result } = renderHook(() => useNavRailCollapse());
  expect(result.current.collapsed).toBe(true);
});
