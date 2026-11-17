import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { appClient } from "../lib/appClient";
import { isMac } from "../lib/platform";

const MODE_KEY = "pipeline.ui.navRailMode";
/** Older builds stored only whether the rail was hidden. */
const LEGACY_KEY = "pipeline.ui.navRailCollapsed";
/** Below this window width the sidebar folds to icons unless told otherwise. */
export const NAV_RAIL_NARROW_QUERY = "(max-width: 1179px)";

type NavRailMode = "auto" | "expanded" | "collapsed";

function readMode(): NavRailMode {
  try {
    const stored = window.localStorage.getItem(MODE_KEY);
    if (stored === "expanded" || stored === "collapsed") return stored;
    if (stored === "auto") return "auto";
    return window.localStorage.getItem(LEGACY_KEY) === "true"
      ? "collapsed"
      : "auto";
  } catch {
    return "auto";
  }
}

const narrowQuery = () =>
  typeof window.matchMedia === "function"
    ? window.matchMedia(NAV_RAIL_NARROW_QUERY)
    : null;

function subscribeNarrow(notify: () => void) {
  const query = narrowQuery();
  query?.addEventListener("change", notify);
  return () => query?.removeEventListener("change", notify);
}

/**
 * Whether the primary navigation is folded to icons. It follows the window
 * width until the researcher chooses otherwise; a choice that matches the
 * current width goes back to following it. The persistent title bar owns the
 * toggle; `toggleRef` keeps focus there after a click. The Mac View menu flips
 * the same state and leaves focus where the researcher was working.
 * Settings folds the rail temporarily. Toggles there affect only that visit,
 * leaving the saved preference intact for the rest of the app.
 */
export default function useNavRailCollapse(settingsOpen = false) {
  const [mode, setMode] = useState<NavRailMode>(readMode);
  const [settingsCollapsed, setSettingsCollapsed] = useState(true);
  const narrow = useSyncExternalStore(
    subscribeNarrow,
    () => narrowQuery()?.matches ?? false,
  );
  const collapsed = settingsOpen
    ? settingsCollapsed
    : mode === "auto"
      ? narrow
      : mode === "collapsed";
  const toggleRef = useRef<HTMLButtonElement>(null);
  const toggled = useRef(false);

  useLayoutEffect(() => {
    if (!settingsOpen) setSettingsCollapsed(true);
  }, [settingsOpen]);

  useLayoutEffect(() => {
    if (!toggled.current) return;
    toggled.current = false;
    toggleRef.current?.focus({ preventScroll: true });
  }, [collapsed]);

  const flip = useCallback(() => {
    const next = !collapsed;
    if (settingsOpen) {
      setSettingsCollapsed(next);
      return;
    }
    const nextMode: NavRailMode =
      next === narrow ? "auto" : next ? "collapsed" : "expanded";
    setMode(nextMode);
    try {
      window.localStorage.setItem(MODE_KEY, nextMode);
    } catch {
      /* Optional view preference. */
    }
  }, [collapsed, narrow, settingsOpen]);

  const toggle = useCallback(() => {
    toggled.current = true;
    flip();
  }, [flip]);

  const flipFromMenu = useRef(flip);
  useLayoutEffect(() => {
    flipFromMenu.current = () => {
      toggled.current = document.activeElement === toggleRef.current;
      flip();
    };
  }, [flip]);

  useEffect(() => {
    if (!isMac) return;
    const off = listen("menu:toggle-sidebar", () =>
      flipFromMenu.current(),
    ).catch(() => undefined);
    return () => {
      void off.then((unlisten) => unlisten?.());
    };
  }, []);

  useEffect(() => {
    if (!isMac) return;
    appClient.setSidebarMenuCollapsed(collapsed).catch(() => undefined);
  }, [collapsed]);

  return { collapsed, toggle, toggleRef };
}
