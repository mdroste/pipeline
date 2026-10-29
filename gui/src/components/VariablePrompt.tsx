import { useId, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { VarSpec, InputSlot } from "../lib/types";
import useModalDialog from "../hooks/useModalDialog";

interface Props {
  variables: VarSpec[];
  inputSlots?: InputSlot[];
  /** Values submitted before, restored when a refused launch reopens them. */
  initialValues?: Record<string, string>;
  initialInputs?: Record<string, string>;
  /** Why the last launch with these options failed. */
  error?: string | null;
  onSubmit: (
    values: Record<string, string>,
    inputs: Record<string, string>,
  ) => void;
  onCancel: () => void;
}

const characters = (count: number) =>
  `${count} character${count === 1 ? "" : "s"}`;

/** The backend's rules for a variable value, checked here so a value is
 *  corrected in the dialog instead of failing the launch. The backend stays
 *  authoritative: a pattern this browser cannot read is left to it. */
function variableError(variable: VarSpec, value: string): string | null {
  if (!value) return null;
  const { min_length, max_length, pattern } = variable.validation ?? {};
  const length = Array.from(value).length;
  if (min_length != null && length < min_length)
    return `Enter at least ${characters(min_length)}.`;
  if (max_length != null && length > max_length)
    return `Enter no more than ${characters(max_length)}.`;
  if (pattern) {
    let expression: RegExp | null = null;
    try {
      expression = new RegExp(pattern, "u");
    } catch {
      expression = null;
    }
    if (expression && !expression.test(value))
      return "This value does not match the format the workflow requires.";
  }
  return null;
}

const isChoice = (variable: VarSpec) =>
  variable.kind === "choice" && !!variable.choices?.length;

// A choice list shows only its options, so a value outside them (including
// no default) starts unselected rather than silently showing the first one.
function initialValue(variable: VarSpec): string {
  const value = variable.default ?? "";
  return isChoice(variable) && !variable.choices?.includes(value) ? "" : value;
}

/** Modal shown before a run when the active profile declares variables and/or
 *  extra named inputs. Variables are pre-filled with defaults and submitted as
 *  {var:key}; input slots collect file/folder paths exposed as {input:key}. */
export default function VariablePrompt({
  variables,
  inputSlots = [],
  initialValues,
  initialInputs,
  error: launchError,
  onSubmit,
  onCancel,
}: Props) {
  const idPrefix = useId();
  const titleId = `${idPrefix}-title`;
  const descriptionId = `${idPrefix}-description`;
  const dialogRef = useModalDialog<HTMLDivElement>(onCancel);
  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      variables.map((v) => [v.key, initialValues?.[v.key] ?? initialValue(v)]),
    ),
  );
  const [inputs, setInputs] = useState<Record<string, string>>(() => ({
    ...initialInputs,
  }));
  const [pickerError, setPickerError] = useState<string | null>(null);

  const set = (key: string, val: string) =>
    setValues((prev) => ({ ...prev, [key]: val }));
  const setInput = (key: string, val: string) =>
    setInputs((prev) => ({ ...prev, [key]: val }));

  const pickFile = async (variable: VarSpec) => {
    setPickerError(null);
    try {
      const path = await open({ multiple: false, directory: false });
      if (typeof path === "string") set(variable.key, path);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setPickerError(
        `Could not open the file picker for “${variable.label || variable.key}”: ${message}`,
      );
    }
  };

  const pickInput = async (slot: InputSlot) => {
    setPickerError(null);
    try {
      const path = await open({
        multiple: false,
        directory: slot.mode === "folder",
        ...(slot.mode === "folder"
          ? {}
          : {
              filters: [
                {
                  name: slot.label || "Input",
                  extensions: ["pdf", "tex", "docx"],
                },
              ],
            }),
      });
      if (typeof path === "string") setInput(slot.key, path);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setPickerError(
        `Could not open the ${slot.mode === "folder" ? "folder" : "file"} picker for “${
          slot.label || slot.key
        }”: ${message}`,
      );
    }
  };

  const missingRequired =
    inputSlots.some((s) => s.required && !inputs[s.key]) ||
    variables.some(
      (variable) => variable.required && !(values[variable.key] ?? "").trim(),
    );
  const errors = Object.fromEntries(
    variables.map((v) => [v.key, variableError(v, values[v.key] ?? "")]),
  );
  const invalid = Object.values(errors).some(Boolean);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        className="w-full max-w-md rounded-xl bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 shadow-xl p-5"
      >
        <h3
          id={titleId}
          className="text-base font-semibold text-gray-900 dark:text-gray-100 mb-1"
        >
          Report options
        </h3>
        <p
          id={descriptionId}
          className="text-xs text-gray-500 dark:text-gray-400 mb-4"
        >
          This workflow asks for a few values before generating the report.
        </p>
        {launchError && (
          <p
            role="alert"
            className="mb-3 break-words rounded border border-red-200 bg-red-50 p-2 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300"
          >
            {launchError}
          </p>
        )}
        {pickerError && (
          <p
            role="alert"
            className="mb-3 rounded border border-red-200 bg-red-50 p-2 text-xs text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300"
          >
            {pickerError}
          </p>
        )}
        <div className="space-y-3">
          {variables.map((v) => {
            const error = errors[v.key];
            const errorId = `${idPrefix}-variable-${v.key}-error`;
            const errorProps = {
              "aria-invalid": error ? true : undefined,
              "aria-describedby": error ? errorId : undefined,
            };
            return (
              <div key={v.key}>
                <label
                  htmlFor={`${idPrefix}-variable-${v.key}`}
                  className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1"
                >
                  {v.label || v.key}
                  {v.required && (
                    <span className="text-red-600 dark:text-red-400"> *</span>
                  )}
                </label>
                {isChoice(v) && v.choices ? (
                  <select
                    id={`${idPrefix}-variable-${v.key}`}
                    value={values[v.key] ?? ""}
                    onChange={(e) => set(v.key, e.target.value)}
                    {...errorProps}
                    className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                  >
                    {!v.choices.includes(values[v.key] ?? "") && (
                      <option value="" disabled>
                        Choose…
                      </option>
                    )}
                    {v.choices.map((c) => (
                      <option key={c} value={c}>
                        {c}
                      </option>
                    ))}
                  </select>
                ) : v.kind === "file" ? (
                  <div className="flex gap-2">
                    <input
                      id={`${idPrefix}-variable-${v.key}`}
                      value={values[v.key] ?? ""}
                      onChange={(e) => set(v.key, e.target.value)}
                      {...errorProps}
                      placeholder="path to a file"
                      className="flex-1 py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                    />
                    <button
                      type="button"
                      onClick={() => pickFile(v)}
                      className="px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
                    >
                      Browse…
                    </button>
                  </div>
                ) : (
                  <input
                    id={`${idPrefix}-variable-${v.key}`}
                    type={v.secret ? "password" : "text"}
                    aria-required={v.required || undefined}
                    value={values[v.key] ?? ""}
                    onChange={(e) => set(v.key, e.target.value)}
                    {...errorProps}
                    className="w-full py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                  />
                )}
                {error && (
                  <p
                    id={errorId}
                    className="mt-1 text-xs text-red-600 dark:text-red-400"
                  >
                    {error}
                  </p>
                )}
              </div>
            );
          })}

          {inputSlots.map((s) => (
            <div key={s.key}>
              <label
                htmlFor={`${idPrefix}-input-${s.key}`}
                className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1"
              >
                {s.label || s.key}
                {s.required && (
                  <span className="text-red-600 dark:text-red-400"> *</span>
                )}
                {s.mode === "folder" && (
                  <span className="text-gray-500 dark:text-gray-400">
                    {" "}
                    (folder)
                  </span>
                )}
              </label>
              <div className="flex gap-2">
                <input
                  id={`${idPrefix}-input-${s.key}`}
                  aria-required={s.required || undefined}
                  value={inputs[s.key] ?? ""}
                  onChange={(e) => setInput(s.key, e.target.value)}
                  placeholder={
                    s.mode === "folder" ? "path to a folder" : "path to a file"
                  }
                  className="flex-1 py-2 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-200"
                />
                <button
                  type="button"
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
            type="button"
            onClick={onCancel}
            data-autofocus={
              variables.length === 0 && inputSlots.length === 0
                ? true
                : undefined
            }
            className="px-4 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => onSubmit(values, inputs)}
            disabled={missingRequired || invalid}
            className="px-4 py-2 text-sm rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Continue
          </button>
        </div>
      </div>
    </div>
  );
}
