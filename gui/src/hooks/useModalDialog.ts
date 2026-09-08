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
  inertBackground = false,
): RefObject<T | null> {
  const dialogRef = useRef<T>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    if (!active) return;
    const previousFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const inertSiblings: Array<{ element: HTMLElement; wasInert: boolean }> = [];
    if (inertBackground) {
      // Inert siblings along the whole ancestor path, never the dialog itself.
      // Remember existing values so closing the modal does not enable content
      // that was already disabled by its owner.
      let branch: HTMLElement | null = dialogRef.current;
      while (branch?.parentElement) {
        for (const sibling of branch.parentElement.children) {
          if (sibling === branch || !(sibling instanceof HTMLElement)) continue;
          inertSiblings.push({ element: sibling, wasInert: sibling.hasAttribute("inert") });
          sibling.setAttribute("inert", "");
        }
        branch = branch.parentElement;
        if (branch === document.body) break;
      }
    }
    const focusable = (dialog: HTMLElement) => Array.from(
      dialog.querySelectorAll<HTMLElement>(FOCUSABLE),
    ).filter(control => !control.matches(":disabled") && !control.closest('[hidden], [inert], [aria-hidden="true"]')
      && getComputedStyle(control).display !== "none" && getComputedStyle(control).visibility !== "hidden");
    const focusInitial = window.setTimeout(() => {
      const dialog = dialogRef.current;
      if (!dialog) return;
      const target =
        focusable(dialog).find(control => control.hasAttribute("data-autofocus")) ??
        focusable(dialog)[0] ??
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
      const controls = focusable(dialog);
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
      for (const { element, wasInert } of inertSiblings) {
        if (!wasInert) element.removeAttribute("inert");
      }
      if (previousFocus?.isConnected) previousFocus.focus();
    };
  }, [active, inertBackground]);

  return dialogRef;
}
