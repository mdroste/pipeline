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
      className={`inline-flex max-w-full flex-wrap gap-0.5 rounded-ui-sm bg-sunken p-0.5 ${className}`}
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
            className={`rounded-[4px] px-2.5 py-1 text-ui-label focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:opacity-40 ${
              checked
                ? "bg-raised font-medium text-ink shadow-ui-1"
                : "text-ink-muted hover:text-ink"
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
