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
  THEME_STORAGE_KEY,
  type ThemePreference,
} from "../../lib/theme";

function canCrossfadeTheme() {
  return (
    typeof document.startViewTransition === "function" &&
    !window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function applyTheme(root: HTMLElement, dark: boolean) {
  root.classList.add("theme-switching");
  root.classList.toggle("dark", dark);
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
  const latestDark = useRef(dark);

  const handleThemeChange = useCallback((preference: ThemePreference) => {
    localStorage.setItem(THEME_STORAGE_KEY, preference);
    setTheme(preference);
  }, []);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const changed = latestDark.current !== dark;
    latestDark.current = dark;
    // A crossfade applies its change a frame later, so it reads the latest
    // value: a newer change can skip it, but the older callback still runs.
    const apply = () => applyTheme(root, latestDark.current);
    if (changed && canCrossfadeTheme()) {
      // Ready rejects when the crossfade is skipped, as in a hidden window.
      document.startViewTransition(apply).ready.catch(() => {});
    } else {
      apply();
    }
  }, [dark]);

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
      .setTheme(theme === "system" ? null : theme)
      .catch((error) => console.warn("Unable to apply window theme:", error));
  }, [theme]);

  return { theme, handleThemeChange };
}
