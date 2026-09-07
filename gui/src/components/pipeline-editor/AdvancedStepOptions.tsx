import { memo, useState } from "react";
import type { InputSlot, StepConfig } from "../../lib/types";
import ArtifactContextEditor from "./ArtifactContextEditor";

function AdvancedStepOptions({
  step,
  otherSteps,
  defaultReportStepIds,
  namedInputs,
  inputMode,
  surveyEnabled,
  conditionStepIds,
  section = "all",
  onChange,
}: {
  step: StepConfig;
  otherSteps: Array<Pick<StepConfig, "id" | "label">>;
  defaultReportStepIds: string[];
  namedInputs: InputSlot[];
  inputMode: string;
  surveyEnabled: boolean;
  conditionStepIds: string[];
  section?: "all" | "inputs" | "execution";
  onChange: (patch: Partial<StepConfig>) => void;
}) {
  const hasAny = !!(
    step.system_prompt || step.context.include.length || step.after?.length || step.run_if || step.for_each
  );
  const [open, setOpen] = useState(
    section !== "all" || !!step.run_if,
  );

  const cond = step.run_if ?? null;
  const condKind = cond?.kind ?? "none";

  const setCondKind = (kind: string) => {
    if (kind === "none") return onChange({ run_if: null });
    if (kind === "output_matches")
      return onChange({ run_if: { kind: "output_matches", step: conditionStepIds[0] ?? "", pattern: "" } });
    return onChange({ run_if: { kind: "survey_path", pointer: "", exists: true } });
  };

  const inputClass =
    "w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-xs font-mono text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200 focus:outline-none focus:ring-1 focus:ring-gray-400";

  return (
    <div>
      {section === "all" && (
        <button
          type="button"
          onClick={() => setOpen(!open)}
          className="text-[11px] text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 flex items-center gap-1"
        >
          <span>{open ? "▾" : "▸"}</span>
          <span>Artifact access &amp; execution rules</span>
          {hasAny && !open && <span className="text-[10px] text-gray-600 dark:text-gray-400 ml-1">set</span>}
        </button>
      )}
      {open && (
        <div className={section === "all"
          ? "mt-2 space-y-3 pl-3 border-l-2 border-gray-200 dark:border-gray-700"
          : "space-y-4"}
        >
          {section !== "execution" && (
            <ArtifactContextEditor
              step={step}
              otherSteps={otherSteps}
              defaultReportStepIds={defaultReportStepIds}
              namedInputs={namedInputs}
              inputMode={inputMode}
              surveyEnabled={surveyEnabled}
              onChange={(context) => onChange({ context })}
            />
          )}

          {section !== "inputs" && (
            <>

          <div>
            <label className="block text-xs font-medium mb-1" htmlFor={`system-prompt-${step.id}`}>Reviewer instructions</label>
            <textarea id={`system-prompt-${step.id}`} rows={5} className={inputClass}
              value={step.system_prompt ?? ""}
              onChange={(event) => onChange({ system_prompt: event.target.value })}
              placeholder="Optional role, evidence standards, and writing instructions" />
            <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">Trusted instructions supplied separately from the task. Codex App Server uses developer instructions. Text is literal; variables are not expanded.</p>
          </div>

          {/* Order-only dependencies */}
          <div>
            <label className="block text-[10px] font-medium text-gray-500 mb-0.5">
              Wait for (order only)
            </label>
            <p className="text-[10px] text-gray-600 dark:text-gray-400 mb-1">
              Adds timing constraints without exposing the producer's artifacts.
            </p>
            {otherSteps.length ? (
              <div className="flex flex-wrap gap-1">
                {otherSteps.map((producer) => {
                  const checked = (step.after ?? []).includes(producer.id);
                  return (
                    <label
                      key={producer.id}
                      className={`flex items-center gap-1 text-[10px] px-2 py-1 rounded-md border cursor-pointer ${
                        checked
                          ? "border-gray-500 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-200"
                          : "border-gray-200 dark:border-gray-700 text-gray-500"
                      }`}
                    >
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={(event) => onChange({
                          after: event.target.checked
                            ? [...(step.after ?? []), producer.id]
                            : (step.after ?? []).filter((id) => id !== producer.id),
                        })}
                        className="rounded border-gray-300 dark:border-gray-600"
                      />
                      {producer.label}
                    </label>
                  );
                })}
              </div>
            ) : (
              <p className="text-[10px] text-gray-600 dark:text-gray-400">No other steps.</p>
            )}
          </div>

          {/* run_if condition */}
          <div>
            <label className="block text-[10px] font-medium text-gray-500 mb-0.5">
              Run only if…
            </label>
            <select
              aria-label="Run condition"
              value={condKind}
              onChange={(e) => setCondKind(e.target.value)}
              className={inputClass}
            >
              <option value="none">Always run</option>
              <option value="output_matches" disabled={conditionStepIds.length === 0}>
                An upstream step's output matches a pattern
              </option>
              <option value="survey_path">The survey (orientation) JSON matches</option>
            </select>
            {cond?.kind === "output_matches" && (
              <div className="mt-1.5 space-y-1.5">
                <select
                  aria-label="Condition source step"
                  value={cond.step}
                  onChange={(e) => onChange({ run_if: { ...cond, step: e.target.value } })}
                  className={inputClass}
                >
                  {!conditionStepIds.includes(cond.step) && cond.step && (
                    <option value={cond.step} disabled>{cond.step} (not upstream)</option>
                  )}
                  {conditionStepIds.map((id) => (
                    <option key={id} value={id}>{id}</option>
                  ))}
                </select>
                <input
                  type="text"
                  aria-label="Condition regular expression"
                  value={cond.pattern}
                  onChange={(e) => onChange({ run_if: { ...cond, pattern: e.target.value } })}
                  placeholder="regular expression, e.g. SEVERITY:\s*high"
                  className={inputClass}
                />
                <label className="flex items-center gap-1.5 text-[10px] text-gray-500">
                  <input
                    type="checkbox"
                    checked={!!cond.negate}
                    onChange={(e) => onChange({ run_if: { ...cond, negate: e.target.checked } })}
                  />
                  Invert (run when it does NOT match)
                </label>
              </div>
            )}
            {cond?.kind === "survey_path" && (
              <div className="mt-1.5 space-y-1.5">
                <input
                  type="text"
                  aria-label="Survey JSON pointer"
                  value={cond.pointer}
                  onChange={(e) => onChange({ run_if: { ...cond, pointer: e.target.value } })}
                  placeholder="JSON pointer, e.g. /metadata/paper_type"
                  className={inputClass}
                />
                <input
                  type="text"
                  aria-label="Survey value to equal"
                  value={
                    cond.equals === undefined
                      ? ""
                      : typeof cond.equals === "string"
                        ? cond.equals
                        : JSON.stringify(cond.equals)
                  }
                  onChange={(e) => {
                    const raw = e.target.value;
                    if (!raw) {
                      const { equals: _drop, ...rest } = cond;
                      onChange({ run_if: { ...rest, exists: true } });
                      return;
                    }
                    let val: unknown = raw;
                    try {
                      val = JSON.parse(raw);
                    } catch {
                      /* keep as string */
                    }
                    onChange({ run_if: { kind: "survey_path", pointer: cond.pointer, equals: val } });
                  }}
                  placeholder='equals (optional), e.g. "empirical" or true'
                  className={inputClass}
                />
                <input
                  type="text"
                  aria-label="Survey array value to contain"
                  value={
                    cond.contains === undefined
                      ? ""
                      : typeof cond.contains === "string"
                        ? cond.contains
                        : JSON.stringify(cond.contains)
                  }
                  onChange={(e) => {
                    const raw = e.target.value;
                    if (!raw) {
                      const { contains: _drop, ...rest } = cond;
                      onChange({ run_if: { ...rest, exists: true } });
                      return;
                    }
                    let val: unknown = raw;
                    try {
                      val = JSON.parse(raw);
                    } catch {
                      /* keep as string */
                    }
                    onChange({ run_if: { kind: "survey_path", pointer: cond.pointer, contains: val } });
                  }}
                  placeholder='array contains (optional), e.g. "formal_proofs"'
                  className={inputClass}
                />
                <p className="text-[10px] text-gray-600 dark:text-gray-400">
                  Set either an exact value or an array member. Leave both blank to require only
                  that the pointer exists.
                </p>
              </div>
            )}
          </div>

          {/* Fan-out (map) */}
          <div>
            <label className="flex items-center gap-1.5 text-[10px] font-medium text-gray-500 mb-1">
              <input
                type="checkbox"
                checked={!!step.for_each}
                onChange={(e) =>
                  onChange({ for_each: e.target.checked ? { glob: "*", max: 20 } : null })
                }
              />
              Fan out (run once per item)
            </label>
            {step.for_each && (
              <div className="space-y-1.5 pl-4">
                <select
                  aria-label="Fan-out source type"
                  value={step.for_each.artifact ? "artifact" : "files"}
                  onChange={(e) => onChange({
                    for_each: e.target.value === "artifact"
                      ? {
                          glob: "",
                          max: step.for_each!.max,
                          artifact: { step: otherSteps[0]?.id ?? "", pointer: "" },
                        }
                      : { glob: "*", max: step.for_each!.max, artifact: null },
                  })}
                  className={inputClass}
                >
                  <option value="files">Matching files</option>
                  <option value="artifact" disabled={otherSteps.length === 0}>
                    Upstream JSON array
                  </option>
                </select>
                {step.for_each.artifact ? (
                  <>
                    <select
                      aria-label="Fan-out artifact source"
                      value={step.for_each.artifact.step}
                      onChange={(e) => onChange({
                        for_each: {
                          ...step.for_each!,
                          glob: "",
                          artifact: { ...step.for_each!.artifact!, step: e.target.value },
                        },
                      })}
                      className={inputClass}
                    >
                      {!otherSteps.some(({ id }) => id === step.for_each!.artifact!.step) && (
                        <option value={step.for_each.artifact.step} disabled>
                          {step.for_each.artifact.step} (unavailable)
                        </option>
                      )}
                      {otherSteps.map(({ id, label }) => (
                        <option key={id} value={id}>{label}</option>
                      ))}
                    </select>
                    <input
                      type="text"
                      aria-label="Fan-out JSON pointer"
                      value={step.for_each.artifact.pointer ?? ""}
                      onChange={(e) => onChange({
                        for_each: {
                          ...step.for_each!,
                          glob: "",
                          artifact: { ...step.for_each!.artifact!, pointer: e.target.value },
                        },
                      })}
                      placeholder="JSON pointer, e.g. /findings"
                      className={inputClass}
                    />
                  </>
                ) : (
                  <input
                    type="text"
                    aria-label="Fan-out file pattern"
                    value={step.for_each.glob}
                    onChange={(e) => onChange({
                      for_each: { ...step.for_each!, glob: e.target.value, artifact: null },
                    })}
                    placeholder="glob, e.g. chapters/*.tex or **/*.py"
                    className={inputClass}
                  />
                )}
                <label className="flex items-center gap-2 text-[10px] text-gray-500">
                  {step.for_each.artifact ? "Max items" : "Max files"}
                  <input
                    type="number"
                    aria-label="Maximum fan-out items"
                    min={1}
                    value={step.for_each.max}
                    onChange={(e) =>
                      onChange({
                        for_each: {
                          ...step.for_each!,
                          max: Math.max(1, parseInt(e.target.value, 10) || 1),
                        },
                      })
                    }
                    className={`${inputClass} w-20`}
                  />
                </label>
                <p className="text-[10px] text-gray-600 dark:text-gray-400">
                  Bind <code className="font-mono">{"{item}"}</code> in the prompt to each{" "}
                  {step.for_each.artifact ? "array item" : "file"}. Outputs are merged (enable merge)
                  or read together downstream via{" "}
                  <code className="font-mono">{"{step:" + step.id + "}"}</code>.
                </p>
              </div>
            )}
          </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

// --- Agent selector ---

export default memo(AdvancedStepOptions);
