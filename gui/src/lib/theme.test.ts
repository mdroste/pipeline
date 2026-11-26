import { afterEach, beforeEach, expect, it, vi } from "vitest";
import bootstrap from "../../public/theme-bootstrap.js?raw";
import {
  THEME_OPTIONS,
  THEME_STORAGE_KEY,
  readThemePreference,
  resolveDarkTheme,
  themeOption,
  type ThemePreference,
} from "./theme";

beforeEach(() => {
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({ matches: false })),
  );
  localStorage.clear();
  document.documentElement.className = "";
  delete document.documentElement.dataset.colorTheme;
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function expectBootstrap(preference: ThemePreference, systemDark = false) {
  vi.mocked(window.matchMedia).mockReturnValue({
    matches: systemDark,
  } as MediaQueryList);
  expect(readThemePreference(localStorage)).toBe(preference);
  new Function(bootstrap)();
  const dark = resolveDarkTheme(preference, systemDark);
  expect(document.documentElement.classList.contains("dark")).toBe(dark);
  expect(document.documentElement.style.colorScheme).toBe(
    dark ? "dark" : "light",
  );
  expect(document.documentElement.dataset.colorTheme).toBe(
    themeOption(preference).colorTheme,
  );
}

for (const systemDark of [false, true])
  it.each(THEME_OPTIONS)(
    `bootstraps $label consistently with React (OS dark: ${systemDark})`,
    ({ value }) => {
      localStorage.setItem(THEME_STORAGE_KEY, value);
      expectBootstrap(value, systemDark);
    },
  );

it.each(["light", "dark"] as const)(
  "retains the legacy %s appearance and palette",
  (mode) => {
    localStorage.setItem("theme", mode);
    localStorage.setItem(
      "pipeline.ui.preferences",
      '{"lightColorTheme":"parchment","darkColorTheme":"harbor"}',
    );
    expectBootstrap(mode === "light" ? "parchment-light" : "harbor-dark");
  },
);

it.each([null, "broken", "null", "[]", '{"darkColorTheme":"unknown"}'])(
  "keeps the old dark mode when its palette is missing or invalid: %s",
  (raw) => {
    localStorage.setItem("theme", "dark");
    if (raw !== null) localStorage.setItem("pipeline.ui.preferences", raw);
    expectBootstrap("dark");
  },
);

it.each(["system", "light", "dark"] as const)(
  "an explicit %s selection overrides legacy palettes",
  (value) => {
    localStorage.setItem("theme", "dark");
    localStorage.setItem(
      "pipeline.ui.preferences",
      '{"darkColorTheme":"harbor"}',
    );
    localStorage.setItem(THEME_STORAGE_KEY, value);
    expectBootstrap(value, true);
  },
);

it.each(["missing", "harbor", "default-light", "harbor-system", ""])(
  "falls back to System for an unknown selection: %s",
  (value) => {
    localStorage.setItem(THEME_STORAGE_KEY, value);
    expectBootstrap("system", true);
  },
);

it("uses default colors for the old System setting", () => {
  localStorage.setItem("theme", "system");
  localStorage.setItem(
    "pipeline.ui.preferences",
    '{"lightColorTheme":"parchment","darkColorTheme":"harbor"}',
  );
  expectBootstrap("system", true);
});

it("keeps the OS fallback when storage cannot be read", () => {
  vi.spyOn(localStorage, "getItem").mockImplementation(() => {
    throw new Error("denied");
  });
  expectBootstrap("system", true);
});
