import { useCallback, useEffect, useRef } from "react";

interface Props {
  onResize: (width: number) => void;
  min: number;
  max: number;
}

export default function ResizeHandle({ onResize, min, max }: Props) {
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => () => cleanupRef.current?.(), []);

  const handleMouseDown = useCallback(
    (event: React.MouseEvent) => {
      event.preventDefault();
      const parent = (event.target as HTMLElement).parentElement;
      if (!parent) return;
      const startX = event.clientX;
      const startWidth = parent.getBoundingClientRect().width;

      const onMouseMove = (moveEvent: MouseEvent) => {
        const width = Math.min(max, Math.max(min, startWidth + moveEvent.clientX - startX));
        onResize(width);
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
    [onResize, min, max],
  );

  return (
    <div
      onMouseDown={handleMouseDown}
      className="absolute top-0 right-0 w-1.5 h-full cursor-col-resize
                 hover:bg-gray-300 dark:hover:bg-gray-600 active:bg-gray-400 dark:active:bg-gray-500 transition-colors z-10"
    />
  );
}
