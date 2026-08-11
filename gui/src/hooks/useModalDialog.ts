import { useEffect, useRef, type RefObject } from "react";

const FOCUSABLE = [
  "button:not([disabled])",
  "[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

/**
 * Shared keyboard/focus behavior for blocking dialogs.
 *
 * The dialog component supplies the ARIA role and label. This hook focuses the
 * requested/first control, traps Tab within the dialog, closes on Escape, and
 * restores focus to the control that opened it.
 */
export default function useModalDialog<T extends HTMLElement>(
  onClose: () => void,
  active = true,
): RefObject<T | null> {
  const dialogRef = useRef<T>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    if (!active) return;
    const previousFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const focusInitial = window.setTimeout(() => {
      const dialog = dialogRef.current;
      if (!dialog) return;
      const target =
        dialog.querySelector<HTMLElement>("[data-autofocus]") ??
        dialog.querySelector<HTMLElement>(FOCUSABLE) ??
        dialog;
      target.focus();
    }, 0);

    const onKeyDown = (event: KeyboardEvent) => {
      const dialog = dialogRef.current;
      if (!dialog) return;
      if (event.key === "Escape") {
        event.preventDefault();
        closeRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const controls = Array.from(
        dialog.querySelectorAll<HTMLElement>(FOCUSABLE),
      ).filter((control) => control.getAttribute("aria-hidden") !== "true");
      if (controls.length === 0) {
        event.preventDefault();
        dialog.focus();
        return;
      }
      const first = controls[0];
      const last = controls[controls.length - 1];
      const activeElement = document.activeElement;
      if (event.shiftKey && (activeElement === first || !dialog.contains(activeElement))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (activeElement === last || !dialog.contains(activeElement))) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", onKeyDown);
    return () => {
      window.clearTimeout(focusInitial);
      document.removeEventListener("keydown", onKeyDown);
      previousFocus?.focus();
    };
  }, [active]);

  return dialogRef;
}
