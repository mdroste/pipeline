import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { VarSpec, InputSlot } from "../lib/types";

interface Props {
  variables: VarSpec[];
  inputSlots?: InputSlot[];
  onSubmit: (values: Record<string, string>, inputs: Record<string, string>) => void;
  onCancel: () => void;
}

/** Modal shown before a run when the active profile declares variables and/or
 *  extra named inputs. Variables are pre-filled with defaults and submitted as
 *  {var:key}; input slots collect file/folder paths exposed as {input:key}. */
export default function VariablePrompt({ variables, inputSlots = [], onSubmit, onCancel }: Props) {
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(variables.map((v) => [v.key, v.default ?? ""]))
  );
  const [inputs, setInputs] = useState<Record<string, string>>({});

  const set = (key: string, val: string) => setValues((prev) => ({ ...prev, [key]: val }));
  const setInput = (key: string, val: string) => setInputs((prev) => ({ ...prev, [key]: val }));

  const pickFile = async (key: string) => {
    try {
      const path = await open({ multiple: false, directory: false });
      if (typeof path === "string") set(key, path);
    } catch {
      /* cancelled */
    }
  };

  const pickInput = async (slot: InputSlot) => {
    try {
      const path = await open({ multiple: false, directory: slot.mode === "folder" });
      if (typeof path === "string") setInput(slot.key, path);
    } catch {
      /* cancelled */
    }
  };

  const missingRequired = inputSlots.some((s) => s.required && !inputs[s.key]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-md rounded-xl bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 shadow-xl p-5">
        <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100 mb-1">Run options</h3>
        <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">
          This profile asks for a few values before running.
        </p>
        <div className="space-y-3">
          {variables.map((v) => (
            <div key={v.key}>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1">
                {v.label || v.key}
              </label>
              {v.kind === "choice" && v.choices && v.choices.length > 0 ? (
                <select
                  value={values[v.key] ?? ""}
                  onChange={(e) => set(v.key, e.target.value)}
                  className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                >
                  {v.choices.map((c) => (
                    <option key={c} value={c}>{c}</option>
                  ))}
                </select>
              ) : v.kind === "file" ? (
                <div className="flex gap-2">
                  <input
                    value={values[v.key] ?? ""}
                    onChange={(e) => set(v.key, e.target.value)}
                    placeholder="path to a file"
                    className="flex-1 py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                  />
                  <button
                    onClick={() => pickFile(v.key)}
                    className="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
                  >
                    Browse…
                  </button>
                </div>
              ) : (
                <input
                  value={values[v.key] ?? ""}
                  onChange={(e) => set(v.key, e.target.value)}
                  className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                />
              )}
            </div>
          ))}

          {inputSlots.map((s) => (
            <div key={s.key}>
              <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1">
                {s.label || s.key}
                {s.required && <span className="text-red-500"> *</span>}
                {s.mode === "folder" && <span className="text-gray-400"> (folder)</span>}
              </label>
              <div className="flex gap-2">
                <input
                  value={inputs[s.key] ?? ""}
                  onChange={(e) => setInput(s.key, e.target.value)}
                  placeholder={s.mode === "folder" ? "path to a folder" : "path to a file"}
                  className="flex-1 py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                />
                <button
                  onClick={() => pickInput(s)}
                  className="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
                >
                  Browse…
                </button>
              </div>
            </div>
          ))}
        </div>
        <div className="flex justify-end gap-2 mt-5">
          <button
            onClick={onCancel}
            className="px-4 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
          >
            Cancel
          </button>
          <button
            onClick={() => onSubmit(values, inputs)}
            disabled={missingRequired}
            className="px-4 py-2 text-sm rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Run
          </button>
        </div>
      </div>
    </div>
  );
}
