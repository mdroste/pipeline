import { useEffect, useRef } from "react";
import { DRAFT_FLUSH_EVENT } from "../lib/flushDrafts";

/** A false result keeps a native quit/restart from losing an editor's draft. */
export function useDraftFlush(flush: () => boolean) {
  const latest = useRef(flush);
  latest.current = flush;
  useEffect(() => {
    const retain = (event: Event) => {
      if (!latest.current()) event.preventDefault();
    };
    window.addEventListener("pagehide", retain);
    window.addEventListener(DRAFT_FLUSH_EVENT, retain);
    return () => {
      latest.current();
      window.removeEventListener("pagehide", retain);
      window.removeEventListener(DRAFT_FLUSH_EVENT, retain);
    };
  }, []);
}
