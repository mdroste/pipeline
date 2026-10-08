// The one positioning routine for floating layers (popovers, menus, list
// boxes, tooltips). Pure so it can be tested without layout.

export type Side = "top" | "bottom";
export type Align = "start" | "center" | "end";

export interface AnchoredPosition {
  left: number;
  top: number;
  /** Room available on the resolved side; the layer scrolls beyond it. */
  maxHeight: number;
  /** The side actually used after flipping for space. */
  side: Side;
}

interface Box {
  left: number;
  right: number;
  top: number;
  bottom: number;
  width: number;
}

/** Distance kept between a floating layer and the window edge. */
export const VIEWPORT_MARGIN = 12;
const MINIMUM_HEIGHT = 80;

export function computeAnchoredPosition(
  anchor: Box,
  layer: { width: number; height: number },
  viewport: { width: number; height: number },
  side: Side = "bottom",
  align: Align = "start",
  offset = 8,
): AnchoredPosition {
  const above = anchor.top - offset - VIEWPORT_MARGIN;
  const below = viewport.height - anchor.bottom - offset - VIEWPORT_MARGIN;
  const preferred = side === "top" ? above : below;
  const opposite = side === "top" ? below : above;
  // Keep the requested side when the layer fits there, or when the other
  // side is no better.
  const resolved: Side =
    preferred >= layer.height || preferred >= opposite
      ? side
      : side === "top"
        ? "bottom"
        : "top";
  const maxHeight = Math.max(
    MINIMUM_HEIGHT,
    resolved === "top" ? above : below,
  );
  const height = Math.min(layer.height, maxHeight);
  const desired =
    align === "start"
      ? anchor.left
      : align === "end"
        ? anchor.right - layer.width
        : anchor.left + anchor.width / 2 - layer.width / 2;
  const left = Math.max(
    VIEWPORT_MARGIN,
    Math.min(desired, viewport.width - layer.width - VIEWPORT_MARGIN),
  );
  const top =
    resolved === "top"
      ? Math.max(VIEWPORT_MARGIN, anchor.top - offset - height)
      : anchor.bottom + offset;
  return { left, top, maxHeight, side: resolved };
}

export const samePosition = (
  a: AnchoredPosition | null,
  b: AnchoredPosition | null,
) =>
  a === b ||
  (a !== null &&
    b !== null &&
    a.left === b.left &&
    a.top === b.top &&
    a.maxHeight === b.maxHeight &&
    a.side === b.side);
