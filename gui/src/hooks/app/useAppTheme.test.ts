import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useAppTheme } from "./useAppTheme";
import { saveAppPreferences } from "../../lib/appPreferences";
import { THEME_STORAGE_KEY } from "../../lib/theme";

const setWindowTheme = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setTheme: setWindowTheme }),
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
  vi.clearAllMocks();
  reducedMotion = false;
  localStorage.clear();
  root.className = "";
  root.style.colorScheme = "";
  delete root.dataset.colorTheme;
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

it("applies the theme even when the webview skips the crossfade", async () => {
  const skipped = Promise.reject(new DOMException("Skipped", "AbortError"));
  const catchReady = vi.spyOn(skipped, "catch");
  mockCrossfade(skipped);
  const { result } = renderHook(() => useAppTheme());

  act(() => result.current.handleThemeChange("dark"));
  await act(async () => {});

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

it("applies the selected palette and mode together and restores them on remount", () => {
  const first = renderHook(() => useAppTheme());
  act(() => first.result.current.handleThemeChange("harbor-dark"));
  expect(root).toHaveClass("dark");
  expect(root.dataset.colorTheme).toBe("harbor");
  expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("harbor-dark");
  expect(setWindowTheme).toHaveBeenLastCalledWith("dark");
  first.unmount();
  const second = renderHook(() => useAppTheme());
  expect(root.dataset.colorTheme).toBe("harbor");
  expect(root).toHaveClass("dark");
  act(() => second.result.current.handleThemeChange("parchment-light"));
  expect(root.dataset.colorTheme).toBe("parchment");
  expect(root).not.toHaveClass("dark");
  expect(setWindowTheme).toHaveBeenLastCalledWith("light");
});

it("follows OS changes with default colors only in System mode", () => {
  let systemChanged: (event: { matches: boolean }) => void = () => {};
  vi.stubGlobal("matchMedia", () => ({
    matches: false,
    addEventListener: (_event: string, handler: typeof systemChanged) => {
      systemChanged = handler;
    },
    removeEventListener: vi.fn(),
  }));
  const { result } = renderHook(() => useAppTheme());
  expect(setWindowTheme).toHaveBeenLastCalledWith(null);
  act(() => systemChanged({ matches: true }));
  expect(root).toHaveClass("dark");
  expect(root.dataset.colorTheme).toBe("default");
  act(() => result.current.handleThemeChange("parchment-light"));
  act(() => systemChanged({ matches: true }));
  expect(root).not.toHaveClass("dark");
  expect(root.dataset.colorTheme).toBe("parchment");
  act(() => result.current.handleThemeChange("system"));
  expect(root).toHaveClass("dark");
  expect(root.dataset.colorTheme).toBe("default");
});

it("keeps the latest mode and palette when pending crossfades are overtaken", () => {
  const { callbacks, startViewTransition } = mockCrossfade();
  const { result } = renderHook(() => useAppTheme());
  expect(startViewTransition).not.toHaveBeenCalled();
  expect(root.dataset.colorTheme).toBe("default");
  act(() => result.current.handleThemeChange("iris-dark"));
  act(() => result.current.handleThemeChange("harbor-dark"));
  act(() => result.current.handleThemeChange("parchment-light"));
  callbacks.forEach((callback) => callback());
  expect(root).not.toHaveClass("dark");
  expect(root.dataset.colorTheme).toBe("parchment");
  expect(root).not.toHaveClass("theme-switching");
});

it("applies a palette if starting a view transition throws", () => {
  const { startViewTransition } = mockCrossfade();
  startViewTransition.mockImplementation(() => {
    throw new Error("unavailable");
  });
  const { result } = renderHook(() => useAppTheme());
  act(() => result.current.handleThemeChange("iris-dark"));
  expect(root).toHaveClass("dark");
  expect(root.dataset.colorTheme).toBe("iris");
});

it("migrates the old selection before unrelated preferences can replace legacy fields", () => {
  localStorage.setItem("theme", "dark");
  localStorage.setItem(
    "pipeline.ui.preferences",
    '{"darkColorTheme":"harbor"}',
  );
  const first = renderHook(() => useAppTheme());
  expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("harbor-dark");
  act(() => saveAppPreferences({ editorFontSize: 18 }));
  first.unmount();
  renderHook(() => useAppTheme());
  expect(root.dataset.colorTheme).toBe("harbor");
  expect(root).toHaveClass("dark");
});
