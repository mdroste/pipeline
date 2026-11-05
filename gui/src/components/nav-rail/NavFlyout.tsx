import { useEffect, useRef, type ReactNode, type RefObject } from "react";
import Popover from "../../ui/Popover";

/**
 * A section's contents beside the icon rail. Project navigation is portaled
 * into this layer from the Workspace page, so its React events never pass
 * through the Popover. This layer therefore does its own dismissal by DOM
 * position, and counts the menus those rows open as inside.
 */
export default function NavFlyout({
  label,
  anchorRef,
  onClose,
  onOpenPage,
  children,
}: {
  label: string;
  anchorRef: RefObject<HTMLElement | null>;
  onClose: () => void;
  onOpenPage: () => void;
  children: ReactNode;
}) {
  const body = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const onPress = (event: PointerEvent) => {
      const target = event.target as Element | null;
      if (
        body.current?.contains(target) ||
        anchorRef.current?.contains(target) ||
        target?.closest?.('[role="menu"], [role="dialog"]')
      )
        return;
      closeRef.current();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if ((event.target as Element | null)?.closest?.('[role="menu"]')) return;
      anchorRef.current?.focus();
      closeRef.current();
    };
    document.addEventListener("pointerdown", onPress);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onPress);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [anchorRef]);
  return (
    <Popover
      open
      onClose={onClose}
      anchorRef={anchorRef}
      label={label}
      side="right"
      align="start"
      width={264}
      dismissible={false}
      className="p-1.5"
    >
      <div ref={body}>
        <div className="nav-flyout-heading">
          <span>{label}</span>
          <button type="button" onClick={onOpenPage}>
            Open
          </button>
        </div>
        <div className="nav-flyout-body">{children}</div>
      </div>
    </Popover>
  );
}
