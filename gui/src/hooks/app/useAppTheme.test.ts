import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useAppTheme } from "./useAppTheme";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTheme: vi.fn().mockResolvedValue(undefined) }),
}));

const root = document.documentElement;
let reducedMotion = false;

function mockCrossfade(ready: Promise<void> = Promise.resolve()) {
  const callbacks: (() => void)[] = [];
  const startViewTransition = vi.fn((callback: () => void) => {
    callbacks.push(callback);
    return { ready };
  });
  Object.defineProperty(document, "startViewTransition", {
    configurable: true,
    value: startViewTransition,
  });
  return { callbacks, startViewTransition };
}

beforeEach(() => {
  reducedMotion = false;
  localStorage.clear();
  root.className = "";
  root.style.colorScheme = "";
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("reduced-motion") && reducedMotion,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
});

afterEach(() => {
  vi.unstubAllGlobals();
  delete (document as Partial<Document>).startViewTransition;
});

it("flips every color inside one crossfade after the first render", () => {
  const { callbacks, startViewTransition } = mockCrossfade();
  const { result } = renderHook(() => useAppTheme());
  expect(startViewTransition).not.toHaveBeenCalled();
  expect(root.style.colorScheme).toBe("light");

  act(() => result.current.handleThemeChange("dark"));

  expect(startViewTransition).toHaveBeenCalledOnce();
  expect(root).not.toHaveClass("dark");
  callbacks[0]();
  expect(root).toHaveClass("dark");
  expect(root).not.toHaveClass("theme-switching");
  expect(root.style.colorScheme).toBe("dark");
});

it("keeps element transitions off while the theme class flips", () => {
  const { callbacks } = mockCrossfade();
  const { result } = renderHook(() => useAppTheme());
  const seen: boolean[] = [];
  const getStyle = vi.spyOn(window, "getComputedStyle").mockImplementation(
    (element) =>
      ({
        get color() {
          seen.push(
            element === root &&
              root.classList.contains("dark") &&
              root.classList.contains("theme-switching"),
          );
          return "";
        },
      }) as CSSStyleDeclaration,
  );

  act(() => result.current.handleThemeChange("dark"));
  callbacks[0]();

  expect(seen).toEqual([true]);
  getStyle.mockRestore();
});

it("ends on the newest theme when a pending crossfade is overtaken", () => {
  const { callbacks } = mockCrossfade();
  const { result } = renderHook(() => useAppTheme());

  act(() => result.current.handleThemeChange("dark"));
  act(() => result.current.handleThemeChange("light"));
  callbacks.forEach((callback) => callback());

  expect(root).not.toHaveClass("dark");
  expect(root.style.colorScheme).toBe("light");
});

it("ignores a crossfade the webview skips", async () => {
  const skipped = Promise.reject(new DOMException("Skipped", "AbortError"));
  const catchReady = vi.spyOn(skipped, "catch");
  const { callbacks } = mockCrossfade(skipped);
  const { result } = renderHook(() => useAppTheme());

  act(() => result.current.handleThemeChange("dark"));
  callbacks[0]();

  expect(catchReady).toHaveBeenCalledOnce();
  expect(root).toHaveClass("dark");
});

it("switches at once when motion is reduced", () => {
  reducedMotion = true;
  const { startViewTransition } = mockCrossfade();
  const { result } = renderHook(() => useAppTheme());

  act(() => result.current.handleThemeChange("dark"));

  expect(startViewTransition).not.toHaveBeenCalled();
  expect(root).toHaveClass("dark");
});

it("switches at once in a webview without crossfades", () => {
  const { result } = renderHook(() => useAppTheme());

  act(() => result.current.handleThemeChange("dark"));

  expect(root).toHaveClass("dark");
  expect(root).not.toHaveClass("theme-switching");
});
