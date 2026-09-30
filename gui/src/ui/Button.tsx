import type { ButtonHTMLAttributes } from "react";
import { button, dangerButton, linkButton, primaryButton } from "./classes";

/** The one button. New and touched surfaces use this instead of inline
 * utility strings; the variant map matches ui/classes. */
export default function Button({
  variant = "default",
  className = "",
  type = "button",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "primary" | "danger" | "link";
}) {
  const base =
    variant === "primary"
      ? primaryButton
      : variant === "danger"
        ? dangerButton
        : variant === "link"
          ? linkButton
          : button;
  return <button type={type} className={`${base} ${className}`} {...rest} />;
}
