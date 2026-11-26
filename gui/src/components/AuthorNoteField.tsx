import { useEffect, useId, useRef } from "react";
import { AUTHOR_NOTE_MAX_CHARS, authorNoteLength } from "../lib/authorNote";

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
  const length = authorNoteLength(value);
  const invalid = length > AUTHOR_NOTE_MAX_CHARS;
  useEffect(() => {
    if (focusKey > 0) textarea.current?.focus();
  }, [focusKey]);
  return (
    <section
      className="review-instructions"
      aria-label="Additional instructions"
    >
      <div className="review-section-heading">
        <label htmlFor={id}>Additional instructions</label>
        <span>Optional</span>
      </div>
      <textarea
        ref={textarea}
        id={id}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        rows={6}
        aria-invalid={invalid}
        aria-describedby={`${id}-count${invalid ? ` ${id}-error` : ""}`}
        placeholder="What should reviewers focus on?"
      />
      <div className="review-instructions-footer">
        <span id={`${id}-count`} className={invalid ? "text-danger" : ""}>
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
          >
            Clear
          </button>
        )}
      </div>
      {invalid && (
        <p id={`${id}-error`} role="alert" className="text-danger">
          Keep your instructions within 4,000 characters.
        </p>
      )}
    </section>
  );
}
