import type { KeyboardEvent } from "react";

/**
 * Keyboard movement shared by menus and list boxes: arrows, Home, End, and
 * type-ahead on the item's label. Returns true when the key moved focus.
 */
export function moveFocusInList(
  event: KeyboardEvent<HTMLElement>,
  selector: string,
  wrap = false,
): boolean {
  const items = Array.from(
    event.currentTarget.querySelectorAll<HTMLElement>(selector),
  );
  if (!items.length) return false;
  const current = items.indexOf(document.activeElement as HTMLElement);
  const last = items.length - 1;
  let next = -1;
  if (event.key === "ArrowDown")
    next = current >= last ? (wrap ? 0 : last) : current + 1;
  else if (event.key === "ArrowUp")
    next = current <= 0 ? (wrap ? last : 0) : current - 1;
  else if (event.key === "Home") next = 0;
  else if (event.key === "End") next = last;
  else if (
    event.key.length === 1 &&
    event.key !== " " &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.altKey
  ) {
    const letter = event.key.toLowerCase();
    const ordered = [
      ...items.slice(current + 1),
      ...items.slice(0, current + 1),
    ];
    const match = ordered.find((item) =>
      (item.dataset.label ?? item.textContent ?? "")
        .trim()
        .toLowerCase()
        .startsWith(letter),
    );
    next = match ? items.indexOf(match) : -1;
  }
  if (next < 0) return false;
  event.preventDefault();
  items[next].focus();
  return true;
}
