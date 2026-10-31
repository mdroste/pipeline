import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

const STORAGE_KEY = "pipeline.ui.navRailCollapsed";

/**
 * Whether the primary navigation is hidden. Only one toggle exists at a time,
 * in the rail or in the bar that replaces it, so `toggleRef` carries focus
 * across the swap instead of dropping it to the document body.
 */
export default function useNavRailCollapse() {
  const [collapsed, setCollapsed] = useState(
    () => window.localStorage.getItem(STORAGE_KEY) === "true",
  );
  const toggleRef = useRef<HTMLButtonElement>(null);
  const toggled = useRef(false);

  useEffect(() => {
    window.localStorage.setItem(STORAGE_KEY, String(collapsed));
  }, [collapsed]);

  useLayoutEffect(() => {
    if (!toggled.current) return;
    toggled.current = false;
    toggleRef.current?.focus({ preventScroll: true });
  }, [collapsed]);

  const toggle = useCallback(() => {
    toggled.current = true;
    setCollapsed((current) => !current);
  }, []);

  return { collapsed, toggle, toggleRef };
}
