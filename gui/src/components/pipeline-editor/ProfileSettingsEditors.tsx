import { memo, useId, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InputSlot, VarSpec } from "../../lib/types";
import useModalDialog from "../../hooks/useModalDialog";

interface CalibrationDraft {
  addendum: string;
  target_step_id: string;
  target_label: string;
  rejected_count: number;
}

function CalibrateSection({ onAppend }: { onAppend: (stepId: string, text: string) => void }) {
  const [draft, setDraft] = useState<CalibrationDraft | null>(null);
  const [text, setText] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const titleId = useId();
  const descriptionId = useId();
  const textareaId = useId();
  const closeDraft = () => setDraft(null);
  const dialogRef = useModalDialog<HTMLDivElement>(closeDraft, draft !== null);

  const run = async () => {
    setLoading(true);
    setError(null);
    try {
      const d = await invoke<CalibrationDraft>("draft_calibration");
      setDraft(d);
      setText(d.addendum);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div>
      <label className="text-sm font-medium text-gray-700 dark:text-gray-300">Calibrate from feedback</label>
      <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 mb-2 leading-relaxed">
        Draft an instruction from the issues you've rejected (in the Issues view) so this profile stops
        flagging them, and append it to the synthesis step.
      </p>
      <button
        type="button"
        onClick={run}
        disabled={loading}
        className="text-xs px-3 py-1.5 rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 disabled:opacity-50"
      >
        {loading ? "Drafting…" : "Draft from my rejected issues"}
      </button>
      {error && <p className="text-xs text-red-600 dark:text-red-400 mt-1.5">{error}</p>}

      {draft && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
          onMouseDown={(event) => {
            // Close only on a press that starts on the backdrop itself — a
            // click handler would also fire when a text-selection drag that
            // began in the textarea is released over the backdrop, discarding
            // the draft.
            if (event.target === event.currentTarget) setDraft(null);
          }}
        >
          <div
            ref={dialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            aria-describedby={descriptionId}
            tabIndex={-1}
            className="w-full max-w-lg rounded-xl bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 shadow-xl p-5"
          >
            <h3 id={titleId} className="text-base font-semibold text-gray-900 dark:text-gray-100 mb-1">Calibration draft</h3>
            <p id={descriptionId} className="text-xs text-gray-500 dark:text-gray-400 mb-3">
              From {draft.rejected_count} rejected issue{draft.rejected_count === 1 ? "" : "s"}. Will be appended to
              step <span className="font-medium">{draft.target_label}</span>. Edit before applying.
            </p>
            <label htmlFor={textareaId} className="sr-only">Calibration instruction</label>
            <textarea
              id={textareaId}
              data-autofocus
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={6}
              className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
            />
            <div className="flex justify-end gap-2 mt-4">
              <button
                type="button"
                onClick={() => setDraft(null)}
                className="px-4 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => { if (text.trim()) onAppend(draft.target_step_id, text.trim()); setDraft(null); }}
                disabled={!text.trim()}
                className="px-4 py-2 text-sm rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:opacity-90 disabled:opacity-40"
              >
                Append &amp; keep
              </button>
            </div>
            <p className="text-[10px] text-gray-600 dark:text-gray-400 mt-2">Save the profile to persist the change.</p>
          </div>
        </div>
      )}
    </div>
  );
}

// --- Extra named inputs (profile-level) ---
//
// Additional inputs beyond the primary one; each is extracted at run time and
// available for selection by each step. When text is selected, {input:key}
// resolves to the step-private staged path.

function ExtraInputsEditor({
  slots,
  onChange,
}: {
  slots: InputSlot[];
  onChange: (s: InputSlot[]) => void;
}) {
  const update = (i: number, patch: Partial<InputSlot>) =>
    onChange(slots.map((s, j) => (j === i ? { ...s, ...patch } : s)));
  const remove = (i: number) => onChange(slots.filter((_, j) => j !== i));
  const add = () =>
    onChange([...slots, { key: `input${slots.length + 1}`, label: "", mode: "document", required: false }]);

  const inputClass =
    "py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-1 focus:ring-gray-400";

  return (
    <div>
      <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1">
        Extra inputs
      </label>
      <p className="text-[11px] text-gray-500 dark:text-gray-400 mb-2 leading-relaxed">
        Additional files/folders the Generate report action asks for. A step can select the extracted text,
        original source, or both; selected text is available as{" "}
        <code className="font-mono">{"{input:key}"}</code>. Useful for a response letter, rubric,
        or prior report alongside the main input.
      </p>
      <div className="space-y-2">
        {slots.map((s, i) => (
          <div key={i} className="flex flex-wrap items-center gap-1.5 p-2 border border-gray-200 dark:border-gray-700 rounded">
            <input
              aria-label={`Extra input ${i + 1} key`}
              value={s.key}
              onChange={(e) => update(i, { key: e.target.value.replace(/[^a-zA-Z0-9_]/g, "") })}
              placeholder="key"
              className={`${inputClass} w-24 font-mono`}
            />
            <input
              aria-label={`Extra input ${i + 1} label`}
              value={s.label ?? ""}
              onChange={(e) => update(i, { label: e.target.value })}
              placeholder="label"
              className={`${inputClass} flex-1 min-w-[6rem]`}
            />
            <select
              aria-label={`Extra input ${i + 1} mode`}
              value={s.mode ?? "document"}
              onChange={(e) => update(i, { mode: e.target.value })}
              className={inputClass}
            >
              <option value="document">document</option>
              <option value="folder">folder</option>
            </select>
            <label className="flex items-center gap-1 text-[10px] text-gray-500">
              <input
                type="checkbox"
                aria-label={`Require extra input ${s.label || s.key || i + 1}`}
                checked={!!s.required}
                onChange={(e) => update(i, { required: e.target.checked })}
              />
              required
            </label>
            <input
              aria-label={`Allowed extensions for ${s.label || s.key || i + 1}`}
              value={(s.extensions ?? []).join(", ")}
              onChange={(e) => update(i, {
                extensions: e.target.value.split(",").map((value) => value.trim().toLowerCase().replace(/^\./, "")).filter(Boolean),
              })}
              placeholder="extensions: pdf, docx"
              className={`${inputClass} min-w-[10rem] flex-1`}
            />
            <input
              type="number"
              min={0}
              aria-label={`Maximum bytes for ${s.label || s.key || i + 1}`}
              value={s.max_bytes || ""}
              onChange={(e) => update(i, { max_bytes: Number(e.target.value) || 0 })}
              placeholder="max bytes"
              className={`${inputClass} w-28`}
            />
            <select
              aria-label={`Sensitivity for ${s.label || s.key || i + 1}`}
              value={s.sensitivity ?? ""}
              onChange={(e) => update(i, { sensitivity: e.target.value as InputSlot["sensitivity"] })}
              className={inputClass}
            >
              <option value="">internal</option>
              <option value="public">public</option>
              <option value="confidential">confidential</option>
              <option value="secret">secret</option>
            </select>
            <button
              type="button"
              onClick={() => remove(i)}
              aria-label={`Remove extra input ${s.label || s.key || i + 1}`}
              className="text-xs text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300 px-1"
            >
              ✕
            </button>
          </div>
        ))}
      </div>
      <button
        type="button"
        onClick={add}
        className="mt-2 text-xs text-gray-600 dark:text-gray-300 hover:text-gray-900 dark:hover:text-gray-100 border border-dashed border-gray-300 dark:border-gray-600 rounded px-2 py-1"
      >
        + Add input
      </button>
    </div>
  );
}

// --- Run-time variables (profile-level) ---
//
// Declared variables turn a profile into a template: the Run action prompts for
// values, and steps reference them as {var:key}.

function VariablesEditor({
  variables,
  onChange,
}: {
  variables: VarSpec[];
  onChange: (v: VarSpec[]) => void;
}) {
  const update = (i: number, patch: Partial<VarSpec>) =>
    onChange(variables.map((v, j) => (j === i ? { ...v, ...patch } : v)));
  const remove = (i: number) => onChange(variables.filter((_, j) => j !== i));
  const add = () =>
    onChange([...variables, { key: `var${variables.length + 1}`, label: "", kind: "text", default: "" }]);

  const inputClass =
    "py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-1 focus:ring-gray-400";

  return (
    <div>
      <label className="text-sm font-medium text-gray-700 dark:text-gray-300">Variables</label>
      <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5 mb-2">
        Values the Generate report action asks for, substituted into prompts as{" "}
        <code className="font-mono">{"{var:key}"}</code>.
      </p>
      <div className="space-y-2">
        {variables.map((v, i) => (
          <div key={i} className="flex flex-wrap items-center gap-1.5 p-2 border border-gray-200 dark:border-gray-700 rounded">
            <input
              aria-label={`Variable ${i + 1} key`}
              value={v.key}
              onChange={(e) => update(i, { key: e.target.value.replace(/[^a-zA-Z0-9_]/g, "") })}
              placeholder="key"
              className={`${inputClass} w-24 font-mono`}
            />
            <input
              aria-label={`Variable ${i + 1} label`}
              value={v.label ?? ""}
              onChange={(e) => update(i, { label: e.target.value })}
              placeholder="label"
              className={`${inputClass} flex-1 min-w-[6rem]`}
            />
            <select
              aria-label={`Variable ${i + 1} type`}
              value={v.kind ?? "text"}
              onChange={(e) => update(i, { kind: e.target.value })}
              className={inputClass}
            >
              <option value="text">text</option>
              <option value="choice">choice</option>
              <option value="file">file</option>
            </select>
            <input
              aria-label={`Variable ${i + 1} default`}
              value={v.default ?? ""}
              onChange={(e) => update(i, { default: e.target.value })}
              placeholder="default"
              className={`${inputClass} w-24`}
            />
            <label className="flex items-center gap-1 text-[10px] text-gray-500">
              <input
                type="checkbox"
                checked={!!v.required}
                onChange={(e) => update(i, { required: e.target.checked })}
              />
              required
            </label>
            <label className="flex items-center gap-1 text-[10px] text-gray-500">
              <input
                type="checkbox"
                checked={!!v.secret}
                disabled={v.kind === "file"}
                onChange={(e) => update(i, { secret: e.target.checked })}
              />
              secret
            </label>
            <button
              type="button"
              onClick={() => remove(i)}
              aria-label={`Remove variable ${v.label || v.key || i + 1}`}
              className="text-xs text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300 px-1"
            >
              ✕
            </button>
            {v.kind === "choice" && (
              <input
                aria-label={`Variable ${v.label || v.key || i + 1} choices`}
                value={(v.choices ?? []).join(", ")}
                onChange={(e) => update(i, { choices: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) })}
                placeholder="choices, comma-separated"
                className={`${inputClass} w-full`}
              />
            )}
            <div className="grid w-full grid-cols-[6rem_6rem_1fr] gap-1.5">
              <input
                type="number"
                min={0}
                aria-label={`Minimum length for ${v.label || v.key || i + 1}`}
                value={v.validation?.min_length ?? ""}
                onChange={(e) => update(i, { validation: { ...v.validation, min_length: e.target.value === "" ? undefined : Number(e.target.value) } })}
                placeholder="min length"
                className={inputClass}
              />
              <input
                type="number"
                min={0}
                aria-label={`Maximum length for ${v.label || v.key || i + 1}`}
                value={v.validation?.max_length ?? ""}
                onChange={(e) => update(i, { validation: { ...v.validation, max_length: e.target.value === "" ? undefined : Number(e.target.value) } })}
                placeholder="max length"
                className={inputClass}
              />
              <input
                aria-label={`Validation pattern for ${v.label || v.key || i + 1}`}
                value={v.validation?.pattern ?? ""}
                onChange={(e) => update(i, { validation: { ...v.validation, pattern: e.target.value } })}
                placeholder="optional regular expression"
                className={`${inputClass} font-mono`}
              />
            </div>
          </div>
        ))}
      </div>
      <button
        type="button"
        onClick={add}
        className="mt-2 text-xs text-gray-600 dark:text-gray-300 hover:text-gray-900 dark:hover:text-gray-100 border border-dashed border-gray-300 dark:border-gray-600 rounded px-2 py-1"
      >
        + Add variable
      </button>
    </div>
  );
}

// --- Artifact access and advanced execution rules ---

export const MemoizedCalibrateSection = memo(CalibrateSection);
export const MemoizedExtraInputsEditor = memo(ExtraInputsEditor);
export const MemoizedVariablesEditor = memo(VariablesEditor);
