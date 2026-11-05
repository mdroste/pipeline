import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import {
  AUTHOR_NOTE_MAX_CHARS,
  authorNoteLength,
  normalizeAuthorNote,
} from "../lib/authorNote";
import { Icon } from "../ui/icons";

interface Props {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
  focusKey?: number;
}

export default function AuthorNoteField({
  value,
  onChange,
  disabled = false,
  focusKey = 0,
}: Props) {
  const id = useId();
  const textarea = useRef<HTMLTextAreaElement>(null);
  const [expanded, setExpanded] = useState(Boolean(value));
  const [focusPending, setFocusPending] = useState(false);
  const note = normalizeAuthorNote(value);
  const length = authorNoteLength(value);
  const invalid = length > AUTHOR_NOTE_MAX_CHARS;
  useEffect(() => {
    if (focusKey > 0) {
      setExpanded(true);
      setFocusPending(true);
    }
  }, [focusKey]);
  useLayoutEffect(() => {
    if (!expanded || !textarea.current) return;
    textarea.current.style.height = "auto";
    textarea.current.style.height = `${Math.min(320, Math.max(128, textarea.current.scrollHeight))}px`;
    if (focusPending) {
      textarea.current.focus();
      setFocusPending(false);
    }
  }, [expanded, value, focusPending]);

  return (
    <section
      aria-label="Additional instructions"
      className={`overflow-hidden rounded-ui-md border bg-surface transition-colors ${invalid ? "border-danger" : expanded ? "border-line-strong" : "border-line"}`}
    >
      <button
        type="button"
        disabled={disabled}
        aria-expanded={expanded}
        aria-controls={`${id}-editor`}
        onClick={() => {
          setExpanded(!expanded);
          setFocusPending(!expanded);
        }}
        className="flex min-h-11 w-full items-center gap-2 px-3 py-3 text-left text-ui-label font-medium text-ink transition-colors hover:bg-sunken/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent disabled:opacity-50"
      >
        <span className="min-w-0 flex-1">Additional instructions</span>
        <Icon
          name="chevron-down"
          className={`h-3.5 w-3.5 shrink-0 text-ink-muted transition-transform motion-reduce:transition-none ${expanded ? "rotate-180" : ""}`}
        />
      </button>
      <p
        id={`${id}-scope`}
        className="px-3 pb-3 text-ui-meta leading-5 text-ink-muted"
      >
        {note
          ? "Shared with every review agent."
          : "Optional · Guide what reviewers focus on."}
      </p>
      {!expanded && note && (
        <p className="mx-3 mb-3 line-clamp-2 break-words rounded-ui-sm bg-sunken px-2.5 py-2 text-ui-meta leading-5 text-ink">
          {note}
        </p>
      )}
      <div id={`${id}-editor`} hidden={!expanded} className="px-3 pb-3">
        <label htmlFor={`${id}-text`} className="sr-only">
          Additional instructions for reviewers
        </label>
        <div className="overflow-hidden rounded-ui-sm border border-line bg-surface transition-colors focus-within:border-line-strong focus-within:ring-2 focus-within:ring-accent-soft">
          <textarea
            ref={textarea}
            id={`${id}-text`}
            value={value}
            disabled={disabled}
            onChange={(event) => onChange(event.target.value)}
            rows={4}
            aria-invalid={invalid}
            aria-describedby={`${id}-scope ${id}-count${invalid ? ` ${id}-error` : ""}`}
            placeholder="e.g. Focus on the model’s assumptions and potential failure cases."
            className="block min-h-32 w-full resize-none border-0 bg-transparent px-3 pt-2.5 pb-2 text-ui-label leading-6 text-ink placeholder:text-ink-muted focus:outline-none disabled:opacity-50"
          />
          <div className="flex min-h-9 items-center justify-between gap-2 px-3 pb-2 text-ui-meta">
            <span
              id={`${id}-count`}
              className={`tabular-nums ${invalid ? "text-danger" : "text-ink-muted"}`}
            >
              {length.toLocaleString()} / 4,000
            </span>
            {value && (
              <button
                type="button"
                disabled={disabled}
                onClick={() => {
                  onChange("");
                  textarea.current?.focus();
                }}
                className="rounded-ui-sm px-1 py-0.5 text-ink-muted transition-colors hover:bg-sunken hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50"
              >
                Clear
              </button>
            )}
          </div>
        </div>
      </div>
      {invalid && (
        <p
          id={`${id}-error`}
          role="alert"
          className="px-3 pb-3 text-ui-meta leading-5 text-danger"
        >
          Keep your instructions within 4,000 characters.
        </p>
      )}
    </section>
  );
}
