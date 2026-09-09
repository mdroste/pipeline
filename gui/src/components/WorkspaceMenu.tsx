import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";

/** A small, nonmodal action popover that escapes pane clipping. */
export default function WorkspaceMenu({
  label,
  children,
  triggerRef,
  disabled = false,
}: {
  label: string;
  children: ReactNode;
  triggerRef?: RefObject<HTMLButtonElement | null>;
  disabled?: boolean;
}) {
  const ownTrigger = useRef<HTMLButtonElement>(null);
  const trigger = triggerRef ?? ownTrigger;
  const panel = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({ left: 0, top: 0, maxHeight: 400 });
  const id = useId();
  const close = (focus = true) => {
    setOpen(false);
    if (focus) trigger.current?.focus();
  };
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const rect = trigger.current?.getBoundingClientRect();
      if (!rect) return;
      const height = Math.min(
        panel.current?.scrollHeight ?? 400,
        window.innerHeight - 24,
      );
      setPosition({
        left: Math.max(12, Math.min(rect.right - 248, window.innerWidth - 260)),
        top: Math.max(
          12,
          Math.min(rect.bottom + 6, window.innerHeight - height - 12),
        ),
        maxHeight: window.innerHeight - 24,
      });
    };
    place();
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    if (panel.current) observer?.observe(panel.current);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open]);
  useEffect(() => {
    if (!open) return;
    panel.current
      ?.querySelector<HTMLElement>(
        "button:not(:disabled), select:not(:disabled)",
      )
      ?.focus();
    const outside = (event: Event) => {
      if (
        !panel.current?.contains(event.target as Node) &&
        !trigger.current?.contains(event.target as Node)
      )
        close(false);
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("focusin", outside);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("focusin", outside);
    };
  }, [open]);
  return (
    <>
      <button
        ref={trigger}
        type="button"
        aria-label={label}
        title={label}
        aria-expanded={open}
        aria-controls={id}
        disabled={disabled}
        className="workspace-more-button"
        onClick={() => setOpen((value) => !value)}
      >
        ···
      </button>
      {open &&
        createPortal(
          <div
            ref={panel}
            id={id}
            role="group"
            aria-label={label}
            style={position}
            className="workspace-floating-menu"
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                close();
              }
            }}
            onChange={() => close()}
            onClick={(event) => {
              if ((event.target as HTMLElement).closest("button")) close();
            }}
          >
            {children}
          </div>,
          document.body,
        )}
    </>
  );
}
