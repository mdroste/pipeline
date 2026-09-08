import type { ComponentPropsWithoutRef, ReactNode } from "react";
import ResizeHandle from "./ResizeHandle";

interface Props extends ComponentPropsWithoutRef<"aside"> {
  width: number;
  defaultWidth: number;
  min: number;
  max: number;
  onResize: (width: number) => void;
  resizeLabel: string;
  side?: "left" | "right";
  /** Fill an already allocated pane; do not measure or resize its contents. */
  fill?: boolean;
}

/** Shared shell: keep scroll containers inside so the divider stays reachable. */
export default function SidebarPanel({
  width, defaultWidth, min, max, onResize, resizeLabel, side = "left",
  children, className = "", style, fill = false, ...props
}: Props) {
  return (
    <aside
      {...props}
      style={{ ...style, width: fill ? undefined : width }}
      className={`relative flex min-h-0 min-w-0 shrink-0 flex-col border-gray-200/80 bg-white
                  text-gray-900 dark:border-gray-800 dark:bg-gray-900 dark:text-gray-100
                  ${fill ? "flex-1 overflow-hidden" : side === "left" ? "border-r" : "border-l"} ${className}`}
    >
      {children}
      {!fill && <ResizeHandle
        currentWidth={width}
        defaultWidth={defaultWidth}
        min={min}
        max={max}
        onResize={onResize}
        label={resizeLabel}
        edge={side === "left" ? "right" : "left"}
      />}
    </aside>
  );
}

export function SidebarHeader({ title, actions, children, heading = "h2" }: {
  title: string;
  actions?: ReactNode;
  children?: ReactNode;
  heading?: "h1" | "h2";
}) {
  const Heading = heading;
  return (
    <header className="shrink-0 px-5 pb-5 pt-6">
      <div className="flex items-center justify-between gap-3">
        <Heading className="min-w-0 text-xl font-semibold tracking-[-0.02em] text-gray-950 dark:text-gray-50">
          {title}
        </Heading>
        {actions && <div className="flex shrink-0 items-center gap-1">{actions}</div>}
      </div>
      {children}
    </header>
  );
}
