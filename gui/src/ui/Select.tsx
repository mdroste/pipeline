import { useId, useRef, useState, type KeyboardEvent } from "react";
import Popover from "./Popover";
import { moveFocusInList } from "./listKeys";
import { Icon } from "./icons";
import type { Align, Side } from "./anchoredPosition";

export interface SelectOption<T extends string = string> {
  value: T;
  label: string;
  description?: string;
  disabled?: boolean;
}

/**
 * The one single-choice list. A button shows the current choice and opens a
 * themed list box; it replaces the operating system's select menu.
 */
export default function Select<T extends string = string>({
  label,
  labelledBy,
  value,
  options,
  onChange,
  disabled = false,
  placeholder = "Choose…",
  side = "bottom",
  align = "start",
  id,
  className = "",
}: {
  /** Accessible name when no visible label is associated through labelledBy. */
  label?: string;
  labelledBy?: string;
  value: T | "";
  options: SelectOption<T>[];
  onChange: (value: T) => void;
  disabled?: boolean;
  placeholder?: string;
  side?: Side;
  align?: Align;
  id?: string;
  className?: string;
}) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const listId = useId();
  const selected = options.find((option) => option.value === value);
  const autofocus =
    selected && !selected.disabled
      ? selected.value
      : options.find((option) => !option.disabled)?.value;
  const choose = (option: SelectOption<T>) => {
    if (option.disabled) return;
    setOpen(false);
    trigger.current?.focus();
    if (option.value !== value) onChange(option.value);
  };
  const onListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      (document.activeElement as HTMLElement | null)?.click();
      return;
    }
    moveFocusInList(event, '[role="option"]:not([aria-disabled="true"])');
  };
  return (
    <>
      <button
        ref={trigger}
        id={id}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-label={label}
        aria-labelledby={labelledBy}
        disabled={disabled}
        data-value={value}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            setOpen(true);
          }
        }}
        className={`ui-select-trigger flex min-w-0 items-center justify-between gap-2 text-left ${className}`}
      >
        <span
          className={`min-w-0 truncate ${selected ? "" : "text-ink-muted"}`}
        >
          {selected?.label ?? (value || placeholder)}
        </span>
        <Icon
          name="chevron-down"
          className="h-3.5 w-3.5 shrink-0 text-ink-muted"
        />
      </button>
      <Popover
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={trigger}
        label={label ?? "Options"}
        role="listbox"
        side={side}
        align={align}
        width="anchor"
        id={listId}
        closeOnFocusOut
        className="ui-dropdown-menu"
        onKeyDown={onListKeyDown}
      >
        {options.map((option) => (
          <div
            key={option.value}
            role="option"
            tabIndex={-1}
            aria-selected={option.value === value}
            aria-disabled={option.disabled || undefined}
            data-label={option.label}
            data-autofocus={option.value === autofocus ? "" : undefined}
            onClick={() => choose(option)}
            className={`ui-dropdown-item flex cursor-default items-start gap-2 ${
              option.disabled ? "opacity-40" : ""
            }`}
          >
            <span className="min-w-0 flex-1">
              <span className="block truncate">{option.label}</span>
              {option.description && (
                <span className="mt-0.5 block text-ui-meta text-ink-muted">
                  {option.description}
                </span>
              )}
            </span>
            {option.value === value && (
              <Icon name="check" className="mt-px h-4 w-4 shrink-0" />
            )}
          </div>
        ))}
      </Popover>
    </>
  );
}
