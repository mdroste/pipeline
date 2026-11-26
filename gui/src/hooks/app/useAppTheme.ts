import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  readThemePreference,
  resolveDarkTheme,
  themeOption,
  THEME_STORAGE_KEY,
  type ThemePreference,
  type ColorTheme,
} from "../../lib/theme";

function canCrossfadeTheme() {
  return (
    typeof document.startViewTransition === "function" &&
    !window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function applyTheme(root: HTMLElement, dark: boolean, colorTheme: ColorTheme) {
  root.classList.add("theme-switching");
  root.classList.toggle("dark", dark);
  root.dataset.colorTheme = colorTheme;
  root.style.colorScheme = dark ? "dark" : "light";
  // Commit the new colors while transitions are off so that none of them start.
  void getComputedStyle(root).color;
  root.classList.remove("theme-switching");
}

// Desktop theme ownership is separate from useAppAppearance's scale and density.
export function useAppTheme() {
  const [theme, setTheme] = useState<ThemePreference>(() =>
    readThemePreference(localStorage),
  );
  const [systemIsDark, setSystemIsDark] = useState(
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
  const dark = resolveDarkTheme(theme, systemIsDark);
  const { colorTheme, mode } = themeOption(theme);
  const latest = useRef({ dark, colorTheme });

  const handleThemeChange = useCallback((preference: ThemePreference) => {
    localStorage.setItem(THEME_STORAGE_KEY, preference);
    setTheme(preference);
  }, []);

  useEffect(() => {
    // Capture a migrated selection before unrelated preferences are saved.
    // Restricted storage must still permit rendering the legacy appearance.
    try {
      if (localStorage.getItem(THEME_STORAGE_KEY) === null)
        localStorage.setItem(THEME_STORAGE_KEY, theme);
    } catch {
      // Explicit changes report save failures through the Settings control.
    }
  }, [theme]);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const changed =
      latest.current.dark !== dark || latest.current.colorTheme !== colorTheme;
    latest.current = { dark, colorTheme };
    // A crossfade applies its change a frame later, so it reads the latest
    // value: a newer change can skip it, but the older callback still runs.
    const apply = () =>
      applyTheme(root, latest.current.dark, latest.current.colorTheme);
    if (changed && canCrossfadeTheme()) {
      // Theme changes still apply if the webview cannot start the animation.
      try {
        document.startViewTransition(apply).ready.catch(apply);
      } catch {
        apply();
      }
    } else {
      apply();
    }
  }, [dark, colorTheme]);

  // matchMedia is provided by WebKit/WebView2/WebKitGTK, so system appearance
  // changes update the React UI on macOS, Windows, and Linux.
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => setSystemIsDark(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  // Keep native window chrome in step with the web content. Passing null asks
  // Tauri to follow the OS and is supported across desktop platforms.
  useEffect(() => {
    getCurrentWindow()
      .setTheme(mode === "system" ? null : mode)
      .catch((error) => console.warn("Unable to apply window theme:", error));
  }, [mode]);

  return { theme, handleThemeChange };
}
