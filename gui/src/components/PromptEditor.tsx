// Textarea wrapper that adds placeholder chips (click to insert at cursor)
// and live lint warnings for unknown placeholders.

import { useMemo, useRef } from "react";
import {
  findUnknownPlaceholders,
  placeholdersFor,
  type PromptContext,
} from "../lib/pipelineHelpers";

interface Props {
  value: string;
  onChange: (next: string) => void;
  context: PromptContext;
  /** Concise accessible name for the prompt textarea. */
  ariaLabel: string;
  /** Optional rows hint when the textarea isn't height-bound by its parent. */
  rows?: number;
  /** Override the default fill-parent layout when the editor sits in a fixed-height area. */
  fillHeight?: boolean;
}

export default function PromptEditor({
  value,
  onChange,
  context,
  ariaLabel,
  rows,
  fillHeight = true,
}: Props) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const placeholders = useMemo(() => placeholdersFor(context), [context]);
  const lintHits = useMemo(() => findUnknownPlaceholders(value, context), [value, context]);

  const insertAtCursor = (token: string) => {
    const ta = textareaRef.current;
    if (!ta) return;
    const start = ta.selectionStart ?? value.length;
    const end = ta.selectionEnd ?? value.length;
    const next = value.slice(0, start) + token + value.slice(end);
    onChange(next);
    // Restore focus + cursor right after the inserted token in the next tick,
    // otherwise React's controlled re-render snaps the selection to 0.
    requestAnimationFrame(() => {
      const node = textareaRef.current;
      if (!node) return;
      node.focus();
      const pos = start + token.length;
      node.setSelectionRange(pos, pos);
    });
  };

  return (
    <div className={fillHeight ? "flex flex-col h-full min-h-0" : "flex flex-col"}>
      {placeholders.length > 0 && (
        <div className="flex items-center gap-1.5 flex-wrap mb-2 text-[10px]">
          <span className="text-gray-500 dark:text-gray-400 mr-1">Insert:</span>
          {placeholders.map((p) => (
            <button
              key={p.token}
              type="button"
              onClick={() => insertAtCursor(p.token)}
              title={p.description}
              className="font-mono px-1.5 py-0.5 rounded border border-gray-200 dark:border-gray-700
                         bg-white dark:bg-gray-800 text-gray-600 dark:text-gray-300
                         hover:bg-gray-100 dark:hover:bg-gray-700 hover:border-gray-300 dark:hover:border-gray-600
                         transition-colors"
            >
              {p.token}
            </button>
          ))}
        </div>
      )}
      <textarea
        ref={textareaRef}
        aria-label={ariaLabel}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        spellCheck={false}
        rows={rows}
        className={`${fillHeight ? "flex-1 min-h-0" : ""} w-full resize-none font-mono text-xs leading-relaxed
                    p-4 border border-gray-300 dark:border-gray-600 rounded-lg
                    bg-gray-50 dark:bg-gray-800 dark:text-gray-200
                    focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent`}
      />
      {lintHits.length > 0 && (
        <div className="mt-2 px-2 py-1.5 rounded border border-amber-300 dark:border-amber-700
                        bg-amber-50 dark:bg-amber-950/40 text-[11px] text-amber-800 dark:text-amber-300">
          <span className="font-medium">
            {lintHits.length === 1
              ? "1 unknown placeholder"
              : `${lintHits.length} unknown placeholders`}
            :
          </span>{" "}
          <span className="font-mono">
            {lintHits
              .slice(0, 5)
              .map((h) => `${h.match} (line ${h.line})`)
              .join(", ")}
            {lintHits.length > 5 ? ", …" : ""}
          </span>
          <span className="ml-1 text-amber-700/70 dark:text-amber-400/70">
            — they'll be passed to the LLM verbatim.
          </span>
        </div>
      )}
    </div>
  );
}
