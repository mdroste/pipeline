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
    const stored = window.localStorage.getItem(storageKey);
    const parsed = stored === null ? Number.NaN : Number(stored);
    return Number.isFinite(parsed)
      ? clamp(parsed, min, max)
      : clamp(defaultWidth, min, max);
  });

  const setWidth = useCallback(
    (nextWidth: number) => setWidthState(clamp(nextWidth, min, max)),
    [max, min],
  );

  useEffect(() => {
    window.localStorage.setItem(storageKey, String(width));
  }, [storageKey, width]);

  return [width, setWidth] as const;
}
