import { useEffect, useRef, type ReactNode } from "react";
import IconButton from "./IconButton";
import { Icon } from "./icons";

/**
 * A panel that slides over the pane it is placed in, leaving the content
 * behind it visible. The nearest positioned ancestor bounds it.
 */
export default function Sheet({
  open,
  onClose,
  title,
  width = 380,
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  width?: number;
  children: ReactNode;
}) {
  const panel = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!open) return;
    const previous =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    panel.current?.focus({ preventScroll: true });
    return () => previous?.focus({ preventScroll: true });
  }, [open]);
  if (!open) return null;
  return (
    <div className="absolute inset-0 z-40 flex justify-end">
      <div
        data-testid="sheet-scrim"
        className="absolute inset-0 bg-scrim"
        onPointerDown={onClose}
      />
      <section
        ref={panel}
        role="dialog"
        aria-label={title}
        tabIndex={-1}
        style={{ width: `min(${width}px, 100%)` }}
        className="relative flex h-full min-h-0 animate-ui-sheet flex-col border-l border-line bg-surface text-ink shadow-ui-3 outline-none"
        onKeyDown={(event) => {
          if (event.key !== "Escape" || event.defaultPrevented) return;
          event.preventDefault();
          event.stopPropagation();
          onClose();
        }}
      >
        <header className="flex shrink-0 items-center justify-between gap-3 border-b border-line px-4 py-3">
          <h2 className="min-w-0 truncate text-ui-title font-semibold">
            {title}
          </h2>
          <IconButton label="Close" onClick={onClose}>
            <Icon name="close" className="h-4 w-4" />
          </IconButton>
        </header>
        <div className="min-h-0 flex-1 overflow-auto">{children}</div>
      </section>
    </div>
  );
}
