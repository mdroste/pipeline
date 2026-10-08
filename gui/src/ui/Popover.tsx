import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import {
  computeAnchoredPosition,
  samePosition,
  type Align,
  type AnchoredPosition,
  type Side,
} from "./anchoredPosition";

const FOCUSABLE = [
  "button:not(:disabled)",
  "[href]",
  "input:not(:disabled)",
  "select:not(:disabled)",
  "textarea:not(:disabled)",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

export interface PopoverProps {
  open: boolean;
  onClose: () => void;
  /** The control that opened the layer; focus returns here on Escape. */
  anchorRef: RefObject<HTMLElement | null>;
  label: string;
  role?: "dialog" | "menu" | "listbox" | "group";
  side?: Side;
  align?: Align;
  offset?: number;
  /** A fixed width in pixels, or "anchor" to be at least as wide as the anchor. */
  width?: number | "anchor";
  /** Close when keyboard focus moves outside (menus and list boxes). */
  closeOnFocusOut?: boolean;
  /** False while an operation must finish before the layer may close. */
  dismissible?: boolean;
  /** Move focus into the layer when it opens. */
  autoFocus?: boolean;
  id?: string;
  className?: string;
  onKeyDown?: (event: KeyboardEvent<HTMLDivElement>) => void;
  children: ReactNode;
}

/**
 * The one floating layer. It escapes pane clipping through a portal, stays
 * inside the window, flips when there is no room, closes on Escape or an
 * outside press, and hands focus back to its anchor.
 *
 * Nested layers count as inside: React events bubble through portals, so a
 * press in a child popover marks this one as pressed inside too.
 */
export default function Popover({
  open,
  onClose,
  anchorRef,
  label,
  role = "dialog",
  side = "bottom",
  align = "start",
  offset = 8,
  width,
  closeOnFocusOut = false,
  dismissible = true,
  autoFocus = true,
  id,
  className = "",
  onKeyDown,
  children,
}: PopoverProps) {
  const layer = useRef<HTMLDivElement>(null);
  const insidePress = useRef(false);
  const insideFocus = useRef(false);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const dismissibleRef = useRef(dismissible);
  dismissibleRef.current = dismissible;
  const [position, setPosition] = useState<AnchoredPosition | null>(null);
  const [anchorWidth, setAnchorWidth] = useState<number | null>(null);

  useLayoutEffect(() => {
    if (!open) {
      setPosition(null);
      return;
    }
    const place = () => {
      const anchor = anchorRef.current;
      const node = layer.current;
      if (!anchor || !node) return;
      const rect = anchor.getBoundingClientRect();
      setAnchorWidth(rect.width);
      const next = computeAnchoredPosition(
        rect,
        { width: node.offsetWidth, height: node.scrollHeight },
        { width: window.innerWidth, height: window.innerHeight },
        side,
        align,
        offset,
      );
      setPosition((current) => (samePosition(current, next) ? current : next));
    };
    place();
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    if (layer.current) observer?.observe(layer.current);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open, side, align, offset, anchorRef]);

  useEffect(() => {
    if (!open) return;
    const outside = (target: EventTarget | null, inside: boolean) =>
      dismissibleRef.current &&
      !inside &&
      !anchorRef.current?.contains(target as Node);
    const onPress = (event: PointerEvent) => {
      const inside = insidePress.current;
      insidePress.current = false;
      if (outside(event.target, inside)) closeRef.current();
    };
    const onFocus = (event: FocusEvent) => {
      const inside = insideFocus.current;
      insideFocus.current = false;
      if (closeOnFocusOut && outside(event.target, inside)) closeRef.current();
    };
    document.addEventListener("pointerdown", onPress);
    document.addEventListener("focusin", onFocus);
    return () => {
      document.removeEventListener("pointerdown", onPress);
      document.removeEventListener("focusin", onFocus);
    };
  }, [open, anchorRef, closeOnFocusOut]);

  useEffect(() => {
    if (!open || !autoFocus) return;
    const node = layer.current;
    if (!node) return;
    (
      node.querySelector<HTMLElement>("[data-autofocus]") ??
      node.querySelector<HTMLElement>(FOCUSABLE) ??
      node
    ).focus({ preventScroll: true });
  }, [open, autoFocus]);

  if (!open) return null;
  const origin = `${align === "end" ? "right" : align === "center" ? "center" : "left"} ${
    (position?.side ?? side) === "top" ? "bottom" : "top"
  }`;
  return createPortal(
    <div
      ref={layer}
      id={id}
      role={role}
      aria-label={label}
      tabIndex={-1}
      data-side={position?.side ?? side}
      style={{
        left: position?.left ?? 0,
        top: position?.top ?? 0,
        maxHeight: position?.maxHeight,
        width: typeof width === "number" ? width : undefined,
        minWidth: width === "anchor" ? (anchorWidth ?? undefined) : undefined,
        // Stays focusable while unplaced; it is placed before first paint.
        opacity: position ? undefined : 0,
        transformOrigin: origin,
      }}
      className={`fixed z-[90] max-w-[calc(100vw-24px)] animate-ui-pop overflow-y-auto rounded-ui-md border border-line bg-raised text-ui-label text-ink shadow-ui-3 outline-none ${className}`}
      onPointerDownCapture={() => {
        insidePress.current = true;
      }}
      onFocusCapture={() => {
        insideFocus.current = true;
      }}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (event.key !== "Escape" || event.defaultPrevented) return;
        event.stopPropagation();
        if (!dismissible) return;
        event.preventDefault();
        anchorRef.current?.focus();
        onClose();
      }}
    >
      {children}
    </div>,
    document.body,
  );
}
