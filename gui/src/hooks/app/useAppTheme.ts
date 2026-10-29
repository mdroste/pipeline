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

// Desktop theme ownership is separate from useAppAppearance's scale and density.
export function useAppTheme() {
  const [theme, setTheme] = useState<ThemePreference>(() =>
    readThemePreference(localStorage),
  );
  const [systemIsDark, setSystemIsDark] = useState(
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
  const dark = resolveDarkTheme(theme, systemIsDark);
  const themeApplied = useRef(false);

  const handleThemeChange = useCallback((preference: ThemePreference) => {
    localStorage.setItem(THEME_STORAGE_KEY, preference);
    setTheme(preference);
  }, []);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const animate =
      themeApplied.current && root.classList.contains("dark") !== dark;
    themeApplied.current = true;
    root.classList.toggle("theme-transitioning", animate);
    root.classList.toggle("dark", dark);
    root.style.colorScheme = dark ? "dark" : "light";
    if (!animate) return;

    const timer = setTimeout(() => {
      root.classList.remove("theme-transitioning");
    }, 350);
    return () => {
      clearTimeout(timer);
      root.classList.remove("theme-transitioning");
    };
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
