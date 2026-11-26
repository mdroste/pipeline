import { useLayoutEffect } from "react";
import { useAppPreferences } from "../lib/appPreferences";

export function useAppAppearance() {
  const { interfaceScale, density, typeface } = useAppPreferences();
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.style.fontSize = `${(16 * interfaceScale) / 100}px`;
    root.dataset.density = density;
    root.dataset.typeface = typeface;
  }, [interfaceScale, density, typeface]);
}
