// Preset instruction editor: insertable built-in blocks, advisory lint, and a
// collapsed preview of the assembled developer instructions when the preset
// is the one in use. Mirrors PromptEditor's chip + lint layout.

import { memo, useMemo, useRef } from "react";
import type { EffectiveHarness } from "../../lib/workbenchTypes";
import { byteLength, instructionSections, lintHarnessInstructions } from "./harnessHelpers";

interface Props {
  value: string;
  onChange: (next: string) => void;
  readOnly: boolean;
  blocks: Array<{ id: string; label: string; text: string }>;
  /** Effective harness for this conversation; used only when `inUse`. */
  effective: EffectiveHarness | null;
  inUse: boolean;
}

function HarnessInstructionsEditor({ value, onChange, readOnly, blocks, effective, inUse }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const lint = useMemo(() => lintHarnessInstructions(value), [value]);
  const bytes = useMemo(() => byteLength(value), [value]);

  const insertBlock = (text: string) => {
    const ta = textareaRef.current;
    const start = ta?.selectionStart ?? value.length;
    const end = ta?.selectionEnd ?? value.length;
    const before = value.slice(0, start);
    const token = `${before && !before.endsWith("\n") ? "\n\n" : ""}${text}\n`;
    onChange(before + token + value.slice(end));
    // Restore focus + caret after the inserted block on the next tick;
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
    <div className="flex h-full min-h-0 flex-col">
      <p className="mb-2 text-xs text-gray-500 dark:text-gray-400">
        Added after the host preamble on every turn. Say what to separate, what to preserve, and what counts as evidence; Codex's base instructions stay in place.
      </p>
      {!readOnly && blocks.length > 0 && (
        <div className="mb-2 flex flex-wrap items-center gap-1.5 text-[10px]">
          <span className="mr-1 text-gray-500 dark:text-gray-400">Insert built-in block:</span>
          {blocks.map((block) => (
            <button
              key={block.id}
              type="button"
              onClick={() => insertBlock(block.text)}
              title={block.text}
              className="rounded border border-gray-200 bg-white px-1.5 py-0.5 text-gray-600 transition-colors hover:border-gray-300 hover:bg-gray-100 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700"
            >
              {block.label}
            </button>
          ))}
        </div>
      )}
      {readOnly ? (
        <pre
          aria-label="Preset instructions"
          className="min-h-[8rem] flex-1 overflow-auto whitespace-pre-wrap rounded-lg border border-gray-200 bg-gray-50 p-4 text-xs leading-relaxed text-gray-700 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-200"
        >
          {value.trim() || "This preset adds no instructions."}
        </pre>
      ) : (
        <textarea
          ref={textareaRef}
          aria-label="Preset instructions"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          spellCheck
          className="min-h-[8rem] w-full flex-1 resize-none rounded-lg border border-gray-300 bg-gray-50 p-4 text-xs leading-relaxed focus:border-transparent focus:outline-none focus:ring-2 focus:ring-gray-400 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
        />
      )}
      <div className="mt-1 text-[10px] text-gray-400">{Math.round(bytes / 1024 * 10) / 10} KiB</div>
      {lint.length > 0 && (
        <ul className="mt-2 space-y-1">
          {lint.map((hit) => (
            <li
              key={hit.message}
              className={`rounded border px-2 py-1.5 text-[11px] ${
                hit.level === "error"
                  ? "border-red-300 bg-red-50 text-red-800 dark:border-red-800 dark:bg-red-950/40 dark:text-red-300"
                  : "border-amber-300 bg-amber-50 text-amber-800 dark:border-amber-700 dark:bg-amber-950/40 dark:text-amber-300"
              }`}
            >
              {hit.message}
            </li>
          ))}
        </ul>
      )}
      <details className="mt-3 rounded-lg border border-gray-200 dark:border-gray-700">
        <summary className="cursor-pointer px-3 py-2 text-xs font-semibold">Effective instructions for this conversation</summary>
        <div className="space-y-2 border-t border-gray-200 p-3 dark:border-gray-700">
          {inUse && effective ? instructionSections(effective).map((section) => (
            <section key={section.id} className="rounded border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
              <h4 className="border-b border-gray-100 px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-gray-500 dark:border-gray-800">{section.label}</h4>
              <pre className="max-h-40 overflow-auto whitespace-pre-wrap p-2 text-[11px] text-gray-700 dark:text-gray-300">{section.text}</pre>
            </section>
          )) : (
            <p className="text-xs text-gray-500">Use this preset in the conversation to preview the assembled instructions with its recipe and context.</p>
          )}
        </div>
      </details>
    </div>
  );
}

export default memo(HarnessInstructionsEditor);
