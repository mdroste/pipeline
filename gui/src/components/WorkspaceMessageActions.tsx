import WorkspaceIcon from "./WorkspaceIcon";
import { useEffect, useRef, useState } from "react";

/** Copy Markdown verbatim, including equations and fenced code. */
export default function WorkspaceMessageActions({
  text,
  label,
}: {
  text: string;
  label: string;
}) {
  const [status, setStatus] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  const copy = async () => {
    let copied = false;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
    } catch {
      const focused = document.activeElement as HTMLElement | null;
      const selection = window.getSelection();
      const ranges = selection
        ? Array.from({ length: selection.rangeCount }, (_, index) =>
            selection.getRangeAt(index).cloneRange(),
          )
        : [];
      const field = document.createElement("textarea");
      field.value = text;
      field.style.cssText = "position:fixed;opacity:0;pointer-events:none";
      document.body.appendChild(field);
      field.select();
      try {
        copied = document.execCommand("copy");
      } catch {
        /* Show failure below. */
      }
      field.remove();
      focused?.focus({ preventScroll: true });
      if (selection) {
        selection.removeAllRanges();
        ranges.forEach((range) => selection.addRange(range));
      }
    }
    setStatus(copied ? "copied" : "failed");
    if (timer.current) clearTimeout(timer.current);
    if (copied) timer.current = setTimeout(() => setStatus("idle"), 2000);
  };
  return (
    <div className="workspace-copy-actions">
      <button
        type="button"
        aria-label={`Copy ${label}`}
        title="Copy as Markdown"
        onClick={() => void copy()}
        className="workspace-copy-button"
      >
        <WorkspaceIcon name={status === "copied" ? "check" : "copy"} />
        <span>{status === "copied" ? "Copied" : "Copy"}</span>
      </button>
      <span
        role="status"
        className={
          status === "failed"
            ? "text-xs text-red-600 dark:text-red-400"
            : "sr-only"
        }
      >
        {status === "failed"
          ? "Could not copy. Select the text and copy manually."
          : status === "copied"
            ? "Copied to clipboard"
            : ""}
      </span>
    </div>
  );
}
