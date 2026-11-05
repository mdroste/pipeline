import type { InputHTMLAttributes } from "react";
import useDraftText from "../../hooks/useDraftText";

type Props = Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "type" | "value" | "onChange" | "min" | "max"
> & {
  value: number;
  onChange: (value: number) => void;
  min: number;
  max?: number;
};

/**
 * A whole-number input that keeps partial text local while typing and clamps
 * to its range on blur or Enter. Empty or unreadable text reverts.
 */
export function NumberField({ value, onChange, min, max, ...rest }: Props) {
  const draft = useDraftText(String(value), (text) => {
    const parsed = Number(text);
    if (!text.trim() || !Number.isFinite(parsed)) return;
    const next = Math.min(max ?? Infinity, Math.max(min, Math.round(parsed)));
    if (next !== value) onChange(next);
  });
  return <input {...rest} type="number" min={min} max={max} {...draft} />;
}
