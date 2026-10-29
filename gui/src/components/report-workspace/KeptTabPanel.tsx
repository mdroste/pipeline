import {
  Suspense,
  useEffect,
  useLayoutEffect,
  useRef,
  type ReactNode,
} from "react";

/**
 * Remembers the offset of every scroller inside a panel and restores them
 * when the panel is shown again. Browsers drop the scroll offset of content
 * hidden with display:none.
 */
function usePanelScrollMemory(active: boolean) {
  const ref = useRef<HTMLDivElement>(null);
  const offsets = useRef(new Map<Element, [number, number]>());

  useEffect(() => {
    const panel = ref.current;
    if (!panel) return;
    // Scroll events do not bubble; a capturing listener sees every scroller.
    const record = (event: Event) => {
      const target = event.target;
      if (panel.hidden || !(target instanceof Element)) return;
      offsets.current.set(target, [target.scrollTop, target.scrollLeft]);
    };
    panel.addEventListener("scroll", record, true);
    return () => panel.removeEventListener("scroll", record, true);
  }, []);

  useLayoutEffect(() => {
    if (!active) return;
    for (const [element, [top, left]] of offsets.current) {
      if (!element.isConnected) {
        offsets.current.delete(element);
        continue;
      }
      element.scrollTop = top;
      element.scrollLeft = left;
    }
  }, [active]);

  return ref;
}

/**
 * A report tab panel. Its content mounts on the first visit and then stays
 * mounted while hidden, so reading position, find state and filters survive
 * tab switches. The `hidden` attribute also keeps a hidden reader's find bar
 * from answering Ctrl/Cmd-F.
 */
export default function KeptTabPanel({
  active,
  panelProps,
  children,
}: {
  active: boolean;
  panelProps: React.ComponentPropsWithoutRef<"div">;
  /** Omit until the tab has been visited. */
  children?: ReactNode;
}) {
  const ref = usePanelScrollMemory(active);
  return (
    <div ref={ref} {...panelProps} hidden={!active} className="min-h-0 flex-1">
      {children && (
        <Suspense
          fallback={
            <div className="flex h-full items-center justify-center text-sm text-gray-500 dark:text-gray-400">
              Loading workspace…
            </div>
          }
        >
          {children}
        </Suspense>
      )}
    </div>
  );
}
