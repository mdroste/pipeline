import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import useNavRailCollapse from "./useNavRailCollapse";

const { invoke, menuListeners } = vi.hoisted(() => ({
  invoke: vi.fn(() => Promise.resolve()),
  menuListeners: new Set<() => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, listener: () => void) => {
    expect(event).toBe("menu:toggle-sidebar");
    menuListeners.add(listener);
    return Promise.resolve(() => menuListeners.delete(listener));
  },
}));
vi.mock("../lib/platform", () => ({ isMac: true }));

const chooseMenuItem = () =>
  act(() => menuListeners.forEach((listener) => listener()));

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
  invoke.mockClear();
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

it("flips from the View menu like the button and keeps its title true", () => {
  const { result } = renderHook(() => useNavRailCollapse());
  const menuTitle = () => invoke.mock.lastCall?.at(1);
  expect(menuTitle()).toEqual({ collapsed: false });

  chooseMenuItem();
  expect(result.current.collapsed).toBe(true);
  expect(localStorage.getItem("pipeline.ui.navRailMode")).toBe("collapsed");
  expect(menuTitle()).toEqual({ collapsed: true });

  chooseMenuItem();
  expect(result.current.collapsed).toBe(false);
  expect(menuTitle()).toEqual({ collapsed: false });

  resize(true);
  expect(menuTitle()).toEqual({ collapsed: true });
});

it("leaves focus in place when the View menu flips the sidebar", () => {
  const { result } = renderHook(() => useNavRailCollapse());
  const toggleButton = document.createElement("button");
  const composer = document.createElement("textarea");
  document.body.append(toggleButton, composer);
  result.current.toggleRef.current = toggleButton;

  composer.focus();
  chooseMenuItem();
  expect(document.activeElement).toBe(composer);

  act(() => result.current.toggle());
  expect(document.activeElement).toBe(toggleButton);
  toggleButton.remove();
  composer.remove();
});
