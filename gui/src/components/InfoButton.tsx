import { useEffect, useId, useRef, useState } from "react";

interface Props {
  label: string;
  children: React.ReactNode;
}

export default function InfoButton({ label, children }: Props) {
  const [open, setOpen] = useState(false);
  const dialogId = useId();
  const containerRef = useRef<HTMLSpanElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;

    const closeOnOutsideClick = (event: MouseEvent) => {
      if (!containerRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      buttonRef.current?.focus();
    };

    document.addEventListener("mousedown", closeOnOutsideClick);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOnOutsideClick);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  return (
    <span
      ref={containerRef}
      onClick={(event) => event.stopPropagation()}
      className="relative inline-flex shrink-0 align-middle"
    >
      <button
        ref={buttonRef}
        type="button"
        aria-label={`More information about ${label}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={dialogId}
        onClick={() => setOpen((value) => !value)}
        className="inline-flex h-4 w-4 items-center justify-center rounded-full text-gray-400 transition-colors hover:bg-gray-200 hover:text-gray-700 focus:outline-none focus-visible:ring-2 focus-visible:ring-gray-500/40 dark:text-gray-500 dark:hover:bg-gray-700 dark:hover:text-gray-200"
      >
        <svg aria-hidden="true" viewBox="0 0 20 20" fill="currentColor" className="h-3.5 w-3.5">
          <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm.75-10.75a.75.75 0 00-1.5 0v.1a.75.75 0 001.5 0v-.1zm0 2.75a.75.75 0 00-1.5 0v3a.75.75 0 001.5 0v-3z" clipRule="evenodd" />
        </svg>
      </button>
      {open && (
        <span
          id={dialogId}
          role="dialog"
          aria-label={`${label} help`}
          className="absolute left-0 top-full z-40 mt-2 w-72 max-w-[calc(100vw-2rem)] rounded-lg border border-gray-200 bg-white px-3 py-2.5 text-left text-xs font-normal leading-relaxed text-gray-600 shadow-lg dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300"
        >
          {children}
        </span>
      )}
    </span>
  );
}
