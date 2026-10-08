import {
  cloneElement,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
  type Ref,
} from "react";
import { createPortal } from "react-dom";
import {
  computeAnchoredPosition,
  type AnchoredPosition,
  type Side,
} from "./anchoredPosition";

type AnyProps = Record<string, unknown> & { ref?: Ref<HTMLElement> };
const HOVER_DELAY_MS = 350;

/**
 * A themed replacement for the browser's `title` tooltip. It appears after a
 * short hover, or at once on keyboard focus, and describes its one child.
 */
export default function Tooltip({
  label,
  side = "top",
  children,
}: {
  label: ReactNode;
  side?: Side;
  children: ReactElement;
}) {
  const child = children as ReactElement<AnyProps>;
  const anchor = useRef<HTMLElement | null>(null);
  const layer = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<AnchoredPosition | null>(null);
  const id = useId();
  const cancel = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };
  const hide = () => {
    cancel();
    setOpen(false);
  };
  useEffect(() => cancel, []);
  // A control that becomes disabled stops reporting the pointer leaving.
  useEffect(() => {
    if (open && anchor.current?.matches(":disabled")) hide();
  });
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    document.addEventListener("keydown", onKey);
    window.addEventListener("scroll", hide, true);
    return () => {
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", hide, true);
    };
  }, [open]);
  useLayoutEffect(() => {
    if (!open) {
      setPosition(null);
      return;
    }
    if (!anchor.current || !layer.current) return;
    setPosition(
      computeAnchoredPosition(
        anchor.current.getBoundingClientRect(),
        {
          width: layer.current.offsetWidth,
          height: layer.current.offsetHeight,
        },
        { width: window.innerWidth, height: window.innerHeight },
        side,
        "center",
        6,
      ),
    );
  }, [open, side, label]);

  const call = (name: string, event: unknown) => {
    const handler = child.props[name];
    if (typeof handler === "function") (handler as (e: unknown) => void)(event);
  };
  const setAnchor = (node: HTMLElement | null) => {
    anchor.current = node;
    const original = child.props.ref;
    if (typeof original === "function") original(node);
    else if (original && typeof original === "object")
      (original as { current: HTMLElement | null }).current = node;
  };
  if (!label) return children;
  return (
    <>
      {cloneElement(child, {
        ref: setAnchor,
        "aria-describedby": open
          ? id
          : (child.props["aria-describedby"] as string | undefined),
        onMouseEnter: (event: unknown) => {
          call("onMouseEnter", event);
          cancel();
          timer.current = setTimeout(() => setOpen(true), HOVER_DELAY_MS);
        },
        onMouseLeave: (event: unknown) => {
          call("onMouseLeave", event);
          hide();
        },
        onFocus: (event: unknown) => {
          call("onFocus", event);
          let visible = true;
          try {
            visible = anchor.current?.matches(":focus-visible") ?? true;
          } catch {
            /* Older engines: treat focus as keyboard focus. */
          }
          if (visible) setOpen(true);
        },
        onBlur: (event: unknown) => {
          call("onBlur", event);
          hide();
        },
        onPointerDown: (event: unknown) => {
          call("onPointerDown", event);
          hide();
        },
      })}
      {open &&
        createPortal(
          <div
            ref={layer}
            id={id}
            role="tooltip"
            style={{
              left: position?.left ?? 0,
              top: position?.top ?? 0,
              visibility: position ? undefined : "hidden",
            }}
            className="pointer-events-none fixed z-[120] max-w-[260px] animate-ui-pop rounded-ui-sm bg-inverse px-2 py-1 text-ui-meta text-on-inverse shadow-ui-2"
          >
            {label}
          </div>,
          document.body,
        )}
    </>
  );
}
