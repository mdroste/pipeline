import { useCallback, useEffect, useRef, useState } from "react";

/**
 * A row action can open a form or preview elsewhere on a long page, where the
 * click appears to do nothing. Call `reveal` from the action to bring that
 * section into view and move focus to it. The section takes `ref` and
 * tabIndex={-1}. It is found after the next render, so it may appear in the same
 * update that calls `reveal`.
 */
export default function useReveal<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [request, setRequest] = useState(0);
  useEffect(() => {
    if (!request) return;
    ref.current?.scrollIntoView?.({ block: "start" });
    ref.current?.focus({ preventScroll: true });
  }, [request]);
  const reveal = useCallback(() => setRequest((count) => count + 1), []);
  return { ref, reveal };
}
