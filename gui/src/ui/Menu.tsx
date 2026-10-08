import {
  createContext,
  useContext,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import Popover from "./Popover";
import { moveFocusInList } from "./listKeys";
import { Icon } from "./icons";
import type { Align, Side } from "./anchoredPosition";

const MenuContext = createContext<{ close: () => void } | null>(null);
const ITEMS = '[role^="menuitem"]:not(:disabled)';

/**
 * The one action menu: arrow keys, Home/End and type-ahead move between
 * items; choosing an item closes the menu and returns focus to its trigger.
 */
export function Menu({
  open,
  onClose,
  anchorRef,
  label,
  side = "bottom",
  align = "end",
  width,
  id,
  children,
}: {
  open: boolean;
  onClose: () => void;
  anchorRef: RefObject<HTMLElement | null>;
  label: string;
  side?: Side;
  align?: Align;
  width?: number;
  id?: string;
  children: ReactNode;
}) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    moveFocusInList(event, ITEMS, true);
  };
  return (
    <Popover
      open={open}
      onClose={onClose}
      anchorRef={anchorRef}
      label={label}
      role="menu"
      side={side}
      align={align}
      width={width}
      id={id}
      closeOnFocusOut
      className="min-w-[200px] p-1.5"
      onKeyDown={onKeyDown}
    >
      <MenuContext.Provider
        value={{
          close: () => {
            anchorRef.current?.focus();
            onClose();
          },
        }}
      >
        {children}
      </MenuContext.Provider>
    </Popover>
  );
}

export function MenuItem({
  onSelect,
  icon,
  description,
  checked,
  disabled = false,
  danger = false,
  keepOpen = false,
  children,
}: {
  onSelect?: () => void;
  icon?: ReactNode;
  description?: ReactNode;
  /** Present for a one-of-many choice; renders a check on the chosen item. */
  checked?: boolean;
  disabled?: boolean;
  danger?: boolean;
  /** Leave the menu open, for items that open a further view. */
  keepOpen?: boolean;
  children: ReactNode;
}) {
  const menu = useContext(MenuContext);
  return (
    <button
      type="button"
      role={checked === undefined ? "menuitem" : "menuitemradio"}
      aria-checked={checked}
      disabled={disabled}
      data-label={typeof children === "string" ? children : undefined}
      onClick={() => {
        onSelect?.();
        if (!keepOpen) menu?.close();
      }}
      className={`flex w-full items-start gap-2.5 rounded-ui-sm px-2.5 py-2 text-left text-ui-label outline-none hover:bg-sunken focus:bg-sunken disabled:opacity-40 ${
        danger ? "text-danger" : "text-ink"
      }`}
    >
      {icon !== undefined && (
        <span className="mt-px flex h-[18px] w-[18px] shrink-0 items-center justify-center text-ink-muted">
          {icon}
        </span>
      )}
      <span className="min-w-0 flex-1">
        <span className="block truncate">{children}</span>
        {description && (
          <span className="mt-0.5 block text-ui-meta text-ink-muted">
            {description}
          </span>
        )}
      </span>
      {checked && (
        <span className="mt-px shrink-0 text-ink">
          <Icon name="check" className="h-4 w-4" />
        </span>
      )}
    </button>
  );
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-2.5 pb-1 pt-1.5 text-ui-meta font-medium text-ink-muted">
      {children}
    </div>
  );
}

export function MenuSeparator() {
  return <div role="separator" className="my-1.5 border-t border-line" />;
}
