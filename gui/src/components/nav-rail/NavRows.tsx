import type { ReactNode, Ref } from "react";
import { Icon, type IconName } from "../../ui/icons";
import IconButton from "../../ui/IconButton";

/** A top-level destination. `current` marks this exact page; `active` marks
 *  the section that holds the page when one of its children is current. */
export function NavRow({
  icon,
  label,
  current = false,
  active = false,
  disabled,
  onClick,
  suffix,
  title,
}: {
  icon: IconName;
  label: string;
  current?: boolean;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
  suffix?: ReactNode;
  title?: string;
}) {
  return (
    <button
      type="button"
      className="nav-row"
      aria-current={current ? "page" : undefined}
      data-active={active || undefined}
      disabled={disabled}
      onClick={onClick}
      title={title ?? label}
    >
      <Icon name={icon} />
      <span className="nav-row-label">{label}</span>
      {suffix}
    </button>
  );
}

/** A destination that also holds children. The row opens the destination;
 *  the chevron only shows or hides the children. */
export function NavGroup({
  label,
  open,
  onToggle,
  children,
  ...row
}: Parameters<typeof NavRow>[0] & {
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div className="nav-group">
      <NavRow label={label} {...row} />
      <button
        type="button"
        className="nav-chevron"
        data-open={open || undefined}
        aria-expanded={open}
        aria-label={`${open ? "Collapse" : "Expand"} ${label}`}
        onClick={onToggle}
      >
        <Icon name="chevron-down" className="h-3.5 w-3.5" />
      </button>
      {open && (
        <div className="nav-children" role="group" aria-label={label}>
          {children}
        </div>
      )}
    </div>
  );
}

export function NavChild({
  label,
  current = false,
  strong = false,
  quiet = false,
  disabled,
  onClick,
  suffix,
  title,
  ariaLabel,
  ref,
  ...rest
}: {
  label: string;
  current?: boolean;
  strong?: boolean;
  quiet?: boolean;
  disabled?: boolean;
  onClick: () => void;
  suffix?: ReactNode;
  title?: string;
  /** Says what a row does when its label only names where it is. */
  ariaLabel?: string;
  ref?: Ref<HTMLButtonElement>;
  "aria-haspopup"?: "menu";
  "aria-expanded"?: boolean;
}) {
  return (
    <button
      {...rest}
      ref={ref}
      type="button"
      className="nav-child"
      aria-label={ariaLabel}
      aria-current={current ? "page" : undefined}
      data-strong={strong || undefined}
      data-quiet={quiet || undefined}
      disabled={disabled}
      onClick={onClick}
      title={title ?? label}
    >
      <span className="nav-row-label">{label}</span>
      {suffix}
    </button>
  );
}

/** One destination in the icon rail. Its name appears as a tooltip. */
export function NavIcon({
  icon,
  label,
  tooltip,
  current = false,
  active = false,
  primary = false,
  children,
  ref,
  ...rest
}: {
  icon: IconName;
  label: string;
  tooltip?: string;
  current?: boolean;
  active?: boolean;
  primary?: boolean;
  children?: ReactNode;
  ref?: Ref<HTMLButtonElement>;
  onClick: () => void;
  "aria-expanded"?: boolean;
  "aria-haspopup"?: "menu" | "dialog";
}) {
  return (
    <IconButton
      {...rest}
      ref={ref}
      label={label}
      tooltip={tooltip}
      tooltipSide="right"
      aria-current={current ? "page" : undefined}
      data-active={active || undefined}
      data-primary={primary || undefined}
      className="nav-icon relative"
    >
      <Icon name={icon} />
      {children}
    </IconButton>
  );
}
