// Drag open file tabs into a new order. While a tab is dragged along its strip
// it follows the pointer and its neighbours slide aside through inline
// transforms, so nothing re-renders until the drop commits the new order.
// Dragged off the strip, the tab is handed to `dock`, which can land it in
// another pane or split the editor.

import { useEffect, useRef, type RefObject } from "react";
import { followPointerDrag } from "../../lib/pointerDrag";

/** Distance from the strip's edge, in pixels, that scrolls it during a drag. */
const EDGE = 32;
/** How far the pointer may stray from the strip before the tab leaves it. */
const SLOP = 12;

/** Somewhere a tab dragged off its strip can land. */
export interface TabDock {
  /** The pointer is off the strip at (x, y). */
  move: (path: string, x: number, y: number) => void;
  /** The tab came back to its strip, the drag was cancelled, or it dropped. */
  end: (path: string, drop: boolean) => void;
}

/** `items` with `item` moved to `index`. */
export function movedTo<T>(items: T[], item: T, index: number) {
  const from = items.indexOf(item);
  const to = Math.max(0, Math.min(index, items.length - 1));
  if (from < 0 || from === to) return items;
  const rest = items.filter((_, i) => i !== from);
  rest.splice(to, 0, item);
  return rest;
}

export default function useTabReorder(
  strip: RefObject<HTMLElement | null>,
  tabs: readonly string[],
  onMove: (path: string, index: number) => void,
  dock?: TabDock,
  /** The name shown beside the pointer off the strip; a file's name by default. */
  labelFor?: (path: string) => string,
) {
  const latest = useRef({ tabs, onMove, dock, labelFor });
  latest.current = { tabs, onMove, dock, labelFor };
  const cancel = useRef<(() => void) | null>(null);
  useEffect(() => () => cancel.current?.(), []);

  const onPointerDown = (
    path: string,
    event: React.PointerEvent<HTMLElement>,
  ) => {
    if (event.button !== 0 || !event.isPrimary) return;
    const container = strip.current;
    if (!container) return;
    cancel.current?.();
    const origin =
      event.clientX -
      container.getBoundingClientRect().left +
      container.scrollLeft;
    let drag: {
      order: string[];
      from: number;
      target: number;
      items: HTMLElement[];
      lefts: number[];
      widths: number[];
      slot: number;
      /** Off the strip, waiting for `dock` to place it. */
      docking: boolean;
    } | null = null;
    let clientX = event.clientX;
    let clientY = event.clientY;
    let frame: number | null = null;
    const dock = latest.current.dock;
    const label = document.createElement("div");
    label.className = "file-drag-label";
    label.textContent =
      latest.current.labelFor?.(path) ?? path.split("/").pop() ?? path;

    const begin = () => {
      const order = [...latest.current.tabs];
      const from = order.indexOf(path);
      const items = [...container.children].filter(
        (node): node is HTMLElement =>
          node instanceof HTMLElement && node.classList.contains("file-tab"),
      );
      if (from < 0 || items.length !== order.length) return false;
      if (order.length < 2 && !dock) return false;
      const box = container.getBoundingClientRect();
      const rects = items.map((item) => item.getBoundingClientRect());
      const gap = parseFloat(getComputedStyle(container).columnGap) || 0;
      drag = {
        order,
        from,
        target: from,
        items,
        lefts: rects.map((r) => r.left - box.left + container.scrollLeft),
        widths: rects.map((r) => r.width),
        slot: rects[from].width + gap,
        docking: false,
      };
      container.dataset.reordering = "true";
      items[from].dataset.dragging = "true";
      return true;
    };

    /** Places the dragged tab under the pointer and opens a gap for it. */
    const layout = () => {
      if (!drag) return;
      const { from, items, lefts, widths, slot } = drag;
      const box = container.getBoundingClientRect();
      const away =
        clientY < box.top - SLOP ||
        clientY > box.bottom + SLOP ||
        clientX < box.left - SLOP ||
        clientX > box.right + SLOP;
      if (dock && away) {
        if (!drag.docking) {
          drag.docking = true;
          drag.target = from;
          for (const item of items) item.style.transform = "";
          items[from].dataset.docking = "true";
          document.body.append(label);
        }
        label.style.transform = `translate(${clientX + 14}px, ${clientY + 10}px)`;
        dock.move(path, clientX, clientY);
        return;
      }
      if (drag.docking) {
        drag.docking = false;
        delete items[from].dataset.docking;
        label.remove();
        dock?.end(path, false);
      }
      const last = items.length - 1;
      const offset = Math.max(
        lefts[0] - lefts[from],
        Math.min(
          lefts[last] + widths[last] - lefts[from] - widths[from],
          clientX - box.left + container.scrollLeft - origin,
        ),
      );
      const left = lefts[from] + offset;
      const right = left + widths[from];
      // A neighbour gives way once the dragged tab covers half of it.
      let target = from;
      for (let i = from + 1; i <= last; i++)
        if (right > lefts[i] + widths[i] / 2) target = i;
      for (let i = from - 1; i >= 0; i--)
        if (left < lefts[i] + widths[i] / 2) target = i;
      drag.target = target;
      items.forEach((item, i) => {
        const shift =
          i === from
            ? offset
            : i > from && i <= target
              ? -slot
              : i < from && i >= target
                ? slot
                : 0;
        item.style.transform = shift ? `translateX(${shift}px)` : "";
      });
    };

    /** Scrolls a strip of many tabs while the pointer rests near its edge. */
    const tick = () => {
      frame = null;
      if (!drag) return;
      const box = container.getBoundingClientRect();
      const speed =
        clientX < box.left + EDGE
          ? -Math.ceil((box.left + EDGE - clientX) / 4)
          : clientX > box.right - EDGE
            ? Math.ceil((clientX - box.right + EDGE) / 4)
            : 0;
      const before = container.scrollLeft;
      if (speed) container.scrollLeft = before + speed;
      layout();
      if (speed && container.scrollLeft !== before)
        frame = requestAnimationFrame(tick);
    };

    cancel.current = followPointerDrag(event, {
      start: begin,
      move: (e) => {
        clientX = e.clientX;
        clientY = e.clientY;
        if (frame === null) frame = requestAnimationFrame(tick);
      },
      end: (drop, e) => {
        if (frame !== null) cancelAnimationFrame(frame);
        if (!drag) return;
        if (e) {
          clientX = e.clientX;
          clientY = e.clientY;
        }
        // The last move may still be waiting for its frame.
        if (drop) layout();
        const { order, from, target, items, docking } = drag;
        label.remove();
        if (docking) dock?.end(path, drop);
        // Clear the preview in the same task as the new order renders, so the
        // tabs land in place without sliding back first.
        delete container.dataset.reordering;
        for (const item of items) {
          item.style.transform = "";
          delete item.dataset.dragging;
          delete item.dataset.docking;
        }
        const { tabs, onMove } = latest.current;
        const unchanged =
          tabs.length === order.length && tabs.every((p, i) => p === order[i]);
        if (drop && !docking && unchanged && target !== from)
          onMove(path, target);
      },
    });
  };

  return { onPointerDown };
}
