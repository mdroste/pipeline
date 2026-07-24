import { useCallback, useEffect, useRef } from "react";

interface Props {
  onResize: (width: number) => void;
  min: number;
  max: number;
  currentWidth?: number;
  defaultWidth?: number;
  label?: string;
}

export default function ResizeHandle({
  onResize,
  min,
  max,
  currentWidth,
  defaultWidth,
  label = "Resize panel",
}: Props) {
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => () => cleanupRef.current?.(), []);

  const clamp = useCallback(
    (width: number) => Math.min(max, Math.max(min, width)),
    [max, min],
  );

  const handleMouseDown = useCallback(
    (event: React.MouseEvent) => {
      event.preventDefault();
      (event.currentTarget as HTMLElement).focus();
      const parent = event.currentTarget.parentElement;
      if (!parent) return;
      const startX = event.clientX;
      const startWidth = currentWidth ?? parent.getBoundingClientRect().width;

      const onMouseMove = (moveEvent: MouseEvent) => {
        onResize(clamp(startWidth + moveEvent.clientX - startX));
      };
      const onMouseUp = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        cleanupRef.current = null;
      };
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
      cleanupRef.current = onMouseUp;
    },
    [clamp, currentWidth, onResize],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      const width = currentWidth ?? min;
      const step = event.shiftKey ? 32 : 8;
      let next: number | null = null;
      if (event.key === "ArrowLeft") next = width - step;
      if (event.key === "ArrowRight") next = width + step;
      if (event.key === "Home") next = min;
      if (event.key === "End") next = max;
      if (next === null) return;
      event.preventDefault();
      onResize(clamp(next));
    },
    [clamp, currentWidth, max, min, onResize],
  );

  return (
    <div
      role="separator"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={currentWidth}
      tabIndex={0}
      onMouseDown={handleMouseDown}
      onDoubleClick={() => defaultWidth !== undefined && onResize(clamp(defaultWidth))}
      onKeyDown={handleKeyDown}
      title="Drag to resize · Double-click to reset"
      className="group absolute -right-1 top-0 z-20 h-full w-2 cursor-col-resize
                 focus-visible:outline-none"
    >
      <span
        aria-hidden="true"
        className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-gray-400 opacity-0
                   transition-opacity group-hover:opacity-70 group-active:opacity-100 group-focus-visible:opacity-100
                   dark:bg-gray-500"
      />
    </div>
  );
}
