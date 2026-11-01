import { useRef, type InputHTMLAttributes } from "react";
import useDraftText from "../hooks/useDraftText";

type Props = Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "type" | "value" | "onChange"
> & {
  value: number;
  onChange: (value: number) => void;
};

/**
 * A number input for values that can be negative or fractional. The typed text
 * stays local until blur or Enter: a lone "-" reads as an empty value, and
 * storing that as 0 makes React rewrite the field, which loses the sign. Text
 * the browser cannot read as a number is never saved, and the field reverts.
 */
export default function DecimalField({ value, onChange, ...rest }: Props) {
  const element = useRef<HTMLInputElement>(null);
  const draft = useDraftText(String(value), (text) => {
    if (element.current?.validity.badInput || !text.trim()) return;
    const parsed = Number(text);
    if (Number.isFinite(parsed) && parsed !== value) onChange(parsed);
  });
  return <input {...rest} ref={element} type="number" step="any" {...draft} />;
}
