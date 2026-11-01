import { useEffect, useRef, useState } from "react";

/**
 * Count how often a retained view has come back into view. A retained view
 * stays mounted while hidden, so a load effect keyed on the project alone never
 * reruns; listing this count among its dependencies reloads it on each return,
 * and at no other time. The first visit does not count: mounting already loads.
 */
export default function useReopenCount(active = true): number {
  const [count, setCount] = useState(0);
  const wasActive = useRef(active);
  useEffect(() => {
    if (active && !wasActive.current) setCount((current) => current + 1);
    wasActive.current = active;
  }, [active]);
  return count;
}
