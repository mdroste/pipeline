import { useCallback, useEffect, useState } from "react";

function clamp(width: number, min: number, max: number) {
  return Math.min(max, Math.max(min, width));
}

export default function usePersistentPanelWidth(
  storageKey: string,
  defaultWidth: number,
  min: number,
  max: number,
) {
  const [width, setWidthState] = useState(() => {
    try {
      const stored = window.localStorage.getItem(storageKey);
      const parsed = stored === null ? Number.NaN : Number(stored);
      if (Number.isFinite(parsed)) return clamp(parsed, min, max);
    } catch {
      // Layout remains usable when browser storage is unavailable.
    }
    return clamp(defaultWidth, min, max);
  });

  const setWidth = useCallback(
    (nextWidth: number) => setWidthState(clamp(nextWidth, min, max)),
    [max, min],
  );

  useEffect(() => {
    try {
      window.localStorage.setItem(storageKey, String(width));
    } catch {
      // Keep the resized width in memory if it cannot be persisted.
    }
  }, [storageKey, width]);

  return [width, setWidth] as const;
}
