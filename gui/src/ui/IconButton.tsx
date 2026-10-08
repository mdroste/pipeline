import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import Tooltip from "./Tooltip";
import type { Side } from "./anchoredPosition";

/**
 * A square button that shows only an icon. Its label is both its accessible
 * name and, unless `tooltip` says otherwise, its tooltip.
 */
export default function IconButton({
  label,
  tooltip,
  tooltipSide = "top",
  size = "md",
  className = "",
  type = "button",
  children,
  ref,
  ...rest
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "title"> & {
  label: string;
  /** Longer explanation than the label; pass null for no tooltip. */
  tooltip?: ReactNode;
  tooltipSide?: Side;
  size?: "sm" | "md";
  ref?: Ref<HTMLButtonElement>;
}) {
  return (
    <Tooltip label={tooltip === undefined ? label : tooltip} side={tooltipSide}>
      <button
        ref={ref}
        type={type}
        aria-label={label}
        className={`inline-flex shrink-0 items-center justify-center rounded-ui-sm text-ink-muted transition-colors hover:bg-sunken hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:opacity-40 aria-expanded:bg-sunken aria-expanded:text-ink ${
          size === "sm" ? "h-7 w-7" : "h-8 w-8"
        } ${className}`}
        {...rest}
      >
        {children}
      </button>
    </Tooltip>
  );
}
