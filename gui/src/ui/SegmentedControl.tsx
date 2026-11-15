import { useRef, type KeyboardEvent } from "react";

export interface Segment<T extends string = string> {
  value: T;
  label: string;
  disabled?: boolean;
}

/** A small one-of-many choice shown inline; arrow keys move and choose. */
export default function SegmentedControl<T extends string = string>({
  label,
  value,
  options,
  onChange,
  disabled = false,
  className = "",
}: {
  label: string;
  value: T;
  options: Segment<T>[];
  onChange: (value: T) => void;
  disabled?: boolean;
  className?: string;
}) {
  const group = useRef<HTMLDivElement>(null);
  const enabled = options.filter((option) => !option.disabled);
  const focusable = options.some((option) => option.value === value)
    ? value
    : enabled[0]?.value;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step =
      event.key === "ArrowRight" || event.key === "ArrowDown"
        ? 1
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? -1
          : 0;
    if (!step || disabled || !enabled.length) return;
    event.preventDefault();
    const current = enabled.findIndex((option) => option.value === value);
    const next =
      enabled[(current + step + enabled.length) % enabled.length] ?? enabled[0];
    onChange(next.value);
    Array.from(
      group.current?.querySelectorAll<HTMLElement>('[role="radio"]') ?? [],
    )
      .find((item) => item.dataset.value === next.value)
      ?.focus();
  };
  return (
    <div
      ref={group}
      role="radiogroup"
      aria-label={label}
      aria-disabled={disabled || undefined}
      onKeyDown={onKeyDown}
      className={`ui-segmented ${className}`}
    >
      {options.map((option) => {
        const checked = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={checked}
            data-value={option.value}
            disabled={disabled || option.disabled}
            tabIndex={option.value === focusable ? 0 : -1}
            onClick={() => {
              if (!checked) onChange(option.value);
            }}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
