import { memo } from "react";
import type {
  ArtifactSelector,
  InputSlot,
  NamedInputArtifactPart,
  PrimaryArtifactPart,
  StepArtifactPart,
  StepConfig,
  StepContext,
} from "../../lib/types";
import { defaultStepContext, primaryPartsForMode } from "./utils";

function replaceArtifactSelector(
  include: ArtifactSelector[],
  matches: (selector: ArtifactSelector) => boolean,
  replacement: ArtifactSelector | null,
): ArtifactSelector[] {
  const next = include.filter((selector) => !matches(selector));
  if (replacement) next.push(replacement);
  return next;
}

export function artifactContextSummary(context: StepContext): string {
  const parts: string[] = [];
  for (const selector of context.include) {
    if (selector.kind === "primary") parts.push(`input ${selector.parts.length}`);
    if (selector.kind === "survey") parts.push("survey");
    if (selector.kind === "named_input") parts.push(selector.key);
    if (selector.kind === "step") {
      const labels = [
        selector.parts.includes("report") ? "report" : "",
        selector.parts.includes("files") ? "files" : "",
      ].filter(Boolean).join("+");
      parts.push(`${selector.step} ${labels}`);
    }
  }
  return parts.length ? parts.join(" · ") : "isolated";
}

function ArtifactContextEditor({
  step,
  otherSteps,
  defaultReportStepIds,
  namedInputs,
  inputMode,
  surveyEnabled,
  onChange,
}: {
  step: StepConfig;
  otherSteps: Array<Pick<StepConfig, "id" | "label">>;
  defaultReportStepIds: string[];
  namedInputs: InputSlot[];
  inputMode: string;
  surveyEnabled: boolean;
  onChange: (context: StepContext) => void;
}) {
  const include = step.context?.include ?? [];
  const primary = include.find(
    (selector): selector is Extract<ArtifactSelector, { kind: "primary" }> =>
      selector.kind === "primary",
  );
  const hasSurvey = include.some((selector) => selector.kind === "survey");
  const availablePrimary = primaryPartsForMode(inputMode);

  const setInclude = (next: ArtifactSelector[]) => onChange({ include: next });
  const setPrimaryPart = (part: PrimaryArtifactPart, checked: boolean) => {
    const parts = new Set(primary?.parts ?? []);
    if (checked) parts.add(part);
    else parts.delete(part);
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "primary",
      parts.size ? { kind: "primary", parts: [...parts] } : null,
    ));
  };
  const setSurvey = (checked: boolean) => {
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "survey",
      checked ? { kind: "survey" } : null,
    ));
  };
  const setNamedPart = (
    key: string,
    part: NamedInputArtifactPart,
    checked: boolean,
  ) => {
    const existing = include.find(
      (selector): selector is Extract<ArtifactSelector, { kind: "named_input" }> =>
        selector.kind === "named_input" && selector.key === key,
    );
    const parts = new Set(existing?.parts ?? []);
    if (checked) parts.add(part);
    else parts.delete(part);
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "named_input" && selector.key === key,
      parts.size ? { kind: "named_input", key, parts: [...parts] } : null,
    ));
  };
  const setStepPart = (
    producer: string,
    part: StepArtifactPart,
    checked: boolean,
  ) => {
    const existing = include.find(
      (selector): selector is Extract<ArtifactSelector, { kind: "step" }> =>
        selector.kind === "step" && selector.step === producer,
    );
    const parts = new Set(existing?.parts ?? []);
    if (checked) parts.add(part);
    else parts.delete(part);
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "step" && selector.step === producer,
      parts.size
        ? { kind: "step", step: producer, parts: [...parts], ...(existing?.glob ? { glob: existing.glob } : {}) }
        : null,
    ));
  };
  const setStepGlob = (producer: string, glob: string) => {
    const existing = include.find(
      (selector): selector is Extract<ArtifactSelector, { kind: "step" }> =>
        selector.kind === "step" && selector.step === producer,
    );
    if (!existing) return;
    setInclude(replaceArtifactSelector(
      include,
      (selector) => selector.kind === "step" && selector.step === producer,
      { ...existing, ...(glob ? { glob } : { glob: undefined }) },
    ));
  };

  const standardContext = () =>
    defaultStepContext(
      step.phase,
      inputMode,
      surveyEnabled,
      defaultReportStepIds.map((id) => ({ id, enabled: true })),
    );
  const reportsOnly = (): StepContext => ({
    include: defaultReportStepIds.map((producer) => ({
      kind: "step",
      step: producer,
      parts: ["report"],
    })),
  });

  const checkboxClass = "rounded border-gray-300 dark:border-gray-600";
  const chipClass =
    "text-[10px] px-2 py-1 rounded-md border border-gray-200 dark:border-gray-700 hover:border-gray-400 dark:hover:border-gray-500";

  return (
    <div className="space-y-2.5">
      <div>
        <div className="flex items-center justify-between gap-2">
          <label className="text-[10px] font-semibold uppercase tracking-wide text-gray-500">
            Artifact access
          </label>
          <span className="text-[10px] text-gray-600 dark:text-gray-400 truncate" title={artifactContextSummary(step.context)}>
            {artifactContextSummary(step.context)}
          </span>
        </div>
        <p className="text-[10px] text-gray-600 dark:text-gray-400 mt-0.5 leading-relaxed">
          {step.phase === "parallel"
            ? "This is an exact allowlist. Parallel steps are independent and cannot read another step's output."
            : "This is an exact allowlist. Selecting a step artifact also makes this step wait for its producer."}
        </p>
      </div>

      <div className="flex flex-wrap gap-1">
        <button type="button" onClick={() => onChange(standardContext())} className={chipClass}>
          Standard
        </button>
        <button
          type="button"
          onClick={() => onChange({
            include: availablePrimary.length
              ? [{ kind: "primary", parts: availablePrimary }]
              : [],
          })}
          className={chipClass}
        >
          Input only
        </button>
        {step.phase === "sequential" && (
          <button type="button" onClick={() => onChange(reportsOnly())} className={chipClass}>
            Prior reports
          </button>
        )}
        <button type="button" onClick={() => onChange({ include: [] })} className={chipClass}>
          Isolated
        </button>
      </div>

      <div className="rounded-md border border-gray-200 dark:border-gray-700 divide-y divide-gray-100 dark:divide-gray-800">
        <div className="p-2">
          <p className="text-[10px] font-medium text-gray-600 dark:text-gray-300 mb-1">Primary input</p>
          {availablePrimary.length ? (
            <div className="flex flex-wrap gap-x-3 gap-y-1">
              {([
                ["text", "Readable text"],
                ["structure", "Document structure"],
                ["visuals", "Pages & figures"],
                ["source", "Original source"],
              ] as Array<[PrimaryArtifactPart, string]>)
                .filter(([part]) => availablePrimary.includes(part))
                .map(([part, label]) => (
                  <label key={part} className="flex items-center gap-1 text-[10px] text-gray-600 dark:text-gray-300">
                    <input
                      type="checkbox"
                      className={checkboxClass}
                      checked={primary?.parts.includes(part) ?? false}
                      onChange={(event) => setPrimaryPart(part, event.target.checked)}
                    />
                    {label}
                  </label>
                ))}
            </div>
          ) : (
            <p className="text-[10px] text-gray-600 dark:text-gray-400">This profile has no primary input.</p>
          )}
        </div>

        <label className="flex items-center gap-2 p-2 text-[10px] text-gray-600 dark:text-gray-300">
          <input
            type="checkbox"
            className={checkboxClass}
            checked={hasSurvey}
            disabled={!surveyEnabled}
            onChange={(event) => setSurvey(event.target.checked)}
          />
          Survey / orientation JSON
          {!surveyEnabled && <span className="text-gray-600 dark:text-gray-400">(disabled for profile)</span>}
        </label>

        {namedInputs.map((slot) => {
          const selector = include.find(
            (item): item is Extract<ArtifactSelector, { kind: "named_input" }> =>
              item.kind === "named_input" && item.key === slot.key,
          );
          return (
            <div key={slot.key} className="p-2">
              <p className="text-[10px] font-medium text-gray-600 dark:text-gray-300 mb-1">
                {slot.label || slot.key} <span className="font-mono text-gray-600 dark:text-gray-400">({slot.key})</span>
              </p>
              <div className="flex gap-3">
                {([
                  ["text", "Extracted text"],
                  ["source", "Original source"],
                ] as Array<[NamedInputArtifactPart, string]>).map(([part, label]) => (
                  <label key={part} className="flex items-center gap-1 text-[10px] text-gray-600 dark:text-gray-300">
                    <input
                      type="checkbox"
                      className={checkboxClass}
                      checked={selector?.parts.includes(part) ?? false}
                      onChange={(event) => setNamedPart(slot.key, part, event.target.checked)}
                    />
                    {label}
                  </label>
                ))}
              </div>
            </div>
          );
        })}

        {step.phase === "parallel" && otherSteps.length > 0 && (
          <p className="p-2 text-[10px] text-gray-600 dark:text-gray-400">
            Step reports and supporting files become selectable only in Sequential steps.
          </p>
        )}

        {step.phase === "sequential" && otherSteps.map((producer) => {
          const selector = include.find(
            (item): item is Extract<ArtifactSelector, { kind: "step" }> =>
              item.kind === "step" && item.step === producer.id,
          );
          return (
            <div key={producer.id} className="p-2">
              <div className="flex items-center justify-between gap-2">
                <p className="text-[10px] font-medium text-gray-600 dark:text-gray-300 truncate">
                  {producer.label} <span className="font-mono text-gray-600 dark:text-gray-400">({producer.id})</span>
                </p>
                <div className="flex gap-3 shrink-0">
                  {([
                    ["report", "Report"],
                    ["files", "Files"],
                  ] as Array<[StepArtifactPart, string]>).map(([part, label]) => (
                    <label key={part} className="flex items-center gap-1 text-[10px] text-gray-600 dark:text-gray-300">
                      <input
                        type="checkbox"
                        className={checkboxClass}
                        checked={selector?.parts.includes(part) ?? false}
                        onChange={(event) => setStepPart(producer.id, part, event.target.checked)}
                      />
                      {label}
                    </label>
                  ))}
                </div>
              </div>
              {selector?.parts.includes("files") && (
                <input
                  type="text"
                  aria-label={`File filter for ${producer.label}`}
                  value={selector.glob ?? ""}
                  onChange={(event) => setStepGlob(producer.id, event.target.value)}
                  placeholder="All files, or filter with a glob such as **/*.csv"
                  className="mt-1.5 w-full py-1 px-2 border border-gray-300 dark:border-gray-600 rounded text-[10px] font-mono text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// Artifact access is the core dataflow control. Conditions, output contracts,
// and fan-out remain advanced execution rules in the same compact panel.

/** Schema that makes a step emit a list of issues, enabling the Issues table +
 *  annotations in the report view (Release 1.4). Pair it with the issues
 *  synthesis prompt. */

export default memo(ArtifactContextEditor);
