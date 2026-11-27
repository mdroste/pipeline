// In-app drags that follow the pointer. The desktop shell's native file drop
// handler keeps the webview from seeing HTML drag and drop events on Windows,
// so tabs and file rows track pointer events instead.

/** Pixels the pointer travels before a press becomes a drag. */
const THRESHOLD = 4;

export interface PointerDrag {
  /** The press has become a drag. Return false to leave it a plain click. */
  start?: () => boolean;
  move: (event: PointerEvent) => void;
  /**
   * The drag ended. `drop` is false when it was cancelled by Escape, a lost
   * pointer or the window losing focus; `event` is the release, if any.
   */
  end: (drop: boolean, event?: PointerEvent) => void;
}

/**
 * Follows a primary-button press until it is released. A press that barely
 * moves stays an ordinary click; after a drag, the click the release would
 * cause is swallowed. Returns a function that cancels the drag.
 */
export function followPointerDrag(
  press: {
    pointerId: number;
    clientX: number;
    clientY: number;
    currentTarget: Element;
  },
  drag: PointerDrag,
): () => void {
  const handle = press.currentTarget;
  const { pointerId } = press;
  let started = false;
  let finished = false;
  const previousCursor = document.body.style.cursor;
  const previousSelect = document.body.style.userSelect;

  const move = (event: PointerEvent) => {
    if (event.pointerId !== pointerId) return;
    if (!started) {
      const distance = Math.hypot(
        event.clientX - press.clientX,
        event.clientY - press.clientY,
      );
      if (distance < THRESHOLD) return;
      if (drag.start && !drag.start()) {
        finish(false);
        return;
      }
      started = true;
      document.body.style.cursor = "grabbing";
      document.body.style.userSelect = "none";
      try {
        handle.setPointerCapture?.(pointerId);
      } catch {
        /* The document listeners still follow the pointer. */
      }
    }
    drag.move(event);
  };
  const finish = (drop: boolean, event?: PointerEvent) => {
    if (finished) return;
    finished = true;
    document.removeEventListener("pointermove", move);
    document.removeEventListener("pointerup", up);
    document.removeEventListener("pointercancel", cancel);
    document.removeEventListener("keydown", key, true);
    window.removeEventListener("blur", cancel);
    if (!started) return;
    document.body.style.cursor = previousCursor;
    document.body.style.userSelect = previousSelect;
    if (handle.hasPointerCapture?.(pointerId))
      handle.releasePointerCapture(pointerId);
    // The release would otherwise click whatever the drag started on.
    const swallow = (click: MouseEvent) => {
      click.preventDefault();
      click.stopPropagation();
    };
    window.addEventListener("click", swallow, true);
    setTimeout(() => window.removeEventListener("click", swallow, true));
    drag.end(drop, event);
  };
  const up = (event: PointerEvent) => {
    if (event.pointerId === pointerId) finish(true, event);
  };
  const cancel = (event?: Event) => {
    if (event && "pointerId" in event && event.pointerId !== pointerId) return;
    finish(false);
  };
  const key = (event: KeyboardEvent) => {
    if (event.key !== "Escape" || !started) return;
    event.preventDefault();
    event.stopPropagation();
    cancel();
  };
  document.addEventListener("pointermove", move);
  document.addEventListener("pointerup", up);
  document.addEventListener("pointercancel", cancel);
  document.addEventListener("keydown", key, true);
  window.addEventListener("blur", cancel);
  return () => cancel();
}
