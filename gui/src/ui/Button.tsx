import type { ButtonHTMLAttributes, Ref } from "react";
import {
  button,
  dangerButton,
  linkButton,
  primaryButton,
  quietButton,
} from "./classes";

/** The one button. New and touched surfaces use this instead of inline
 * utility strings; the variant map matches ui/classes. */
export default function Button({
  variant = "default",
  size = "md",
  className = "",
  type = "button",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "primary" | "danger" | "quiet" | "link";
  size?: "sm" | "md";
  ref?: Ref<HTMLButtonElement>;
}) {
  const base =
    variant === "primary"
      ? primaryButton
      : variant === "danger"
        ? dangerButton
        : variant === "link"
          ? linkButton
          : variant === "quiet"
            ? quietButton
            : button;
  return (
    <button
      type={type}
      className={`${base} ${size === "sm" ? "ui-button--sm" : ""} ${className}`}
      {...rest}
    />
  );
}
