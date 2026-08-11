import { useMemo } from "react";
import type { PipelineConfig, StepConfig } from "../lib/types";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";
import useModalDialog from "../hooks/useModalDialog";

export interface RunPreviewPlan {
  profileId: string;
  inputMode: string;
  inputInterpretation: string;
  stages: ExecutionPlanStage[];
}

interface Props {
  plan: RunPreviewPlan;
  config: PipelineConfig;
  inputPath: string;
  batchCount?: number;
  onCancel: () => void;
  onRun: () => void;
}

function basename(path: string): string {
  const parts = path.replaceAll("\\", "/").split("/").filter(Boolean);
  return parts[parts.length - 1] || "No input";
}

function stepProviders(step: StepConfig): string[] {
  return step.agents.length > 0 ? step.agents : ["default provider"];
}

function selectorLabel(selector: StepConfig["context"]["include"][number]): string {
  switch (selector.kind) {
    case "primary":
      return `Primary ${selector.parts.join(", ")}`;
    case "survey":
      return "Orientation map";
    case "named_input":
      return `Named input: ${selector.key}`;
    case "step":
      return `Output: ${selector.step}`;
  }
}

function declaredUnits(step: StepConfig): { minimum: number; maximum: number } {
  const providers = Math.max(step.agents.length, 1);
  const maximumItems = step.for_each ? Math.max(step.for_each.max, 1) : 1;
  return {
    minimum: step.for_each || step.run_if ? 0 : providers,
    maximum: providers * maximumItems,
  };
}

type ContextSelector = StepConfig["context"]["include"][number];

interface AccessRow {
  id: string;
  label: string;
  providers: string[];
  unitLabel: string;
  selectors: ContextSelector[];
  tools: string[];
  conditional?: boolean;
  adaptive?: boolean;
  additionalInputs?: string[];
}

function fixedAccessRow(step: StepConfig): AccessRow {
  const units = declaredUnits(step);
  return {
    id: step.id,
    label: step.label,
    providers: stepProviders(step),
    unitLabel: units.maximum > units.minimum
      ? `up to ${units.maximum} units`
      : `${units.minimum} unit${units.minimum === 1 ? "" : "s"}`,
    selectors: step.context.include,
    tools: step.tools,
    conditional: !!step.run_if,
  };
}

export default function RunPreview({
  plan,
  config,
  inputPath,
  batchCount = 1,
  onCancel,
  onRun,
}: Props) {
  const dialogRef = useModalDialog<HTMLDivElement>(onCancel);
  const steps = config.steps.filter((step) => step.enabled);
  const conditionalSteps = steps.filter((step) => !!step.run_if).length;
  const autoAssembled = config.orientation_schema?.["x-pipeline-contract"] === "auto-review-v2";
  const adaptiveTimelineStages = useMemo(() => {
    let inserted = false;
    return plan.stages.map((stage) => {
      if (!autoAssembled || inserted || stage.kind !== "dispatching") return stage;
      inserted = true;
      return {
        ...stage,
        stepLabels: [
          ...(stage.stepLabels ?? []),
          "Subject specialists (1–2, auto-selected)",
          "Method specialists (1–4, auto-selected)",
        ],
      };
    });
  }, [autoAssembled, plan.stages]);
  const accessRows = useMemo(() => {
    const rows: AccessRow[] = [];
    const specialistContext = steps.find((step) => step.phase === "parallel")?.context.include ?? [];
    let inserted = false;
    const insertAdaptiveRows = () => {
      if (!autoAssembled || inserted) return;
      inserted = true;
      rows.push(
        {
          id: "auto_subject_slot",
          label: "Subject specialists",
          providers: ["default provider"],
          unitLabel: "1–2 units",
          selectors: specialistContext,
          tools: [],
          adaptive: true,
        },
        {
          id: "auto_method_slot",
          label: "Method specialists",
          providers: ["default provider"],
          unitLabel: "1–4 units",
          selectors: specialistContext,
          tools: [],
          adaptive: true,
        },
      );
    };
    for (const step of steps) {
      if (step.id === "auto_synthesis" || step.phase === "sequential") insertAdaptiveRows();
      const row = fixedAccessRow(step);
      if (autoAssembled && step.id === "auto_synthesis") {
        row.additionalInputs = [
          "Reports: selected subject specialists (1–2)",
          "Reports: selected method specialists (1–4)",
        ];
      }
      rows.push(row);
    }
    insertAdaptiveRows();
    return rows;
  }, [autoAssembled, steps]);
  const summary = useMemo(() => {
    const providers = new Set<string>();
    const tools = new Set<string>();
    let minimumUnits = 0;
    let maximumUnits = 0;
    for (const step of steps) {
      stepProviders(step).forEach((provider) => providers.add(provider));
      step.tools.forEach((tool) => tools.add(tool));
      const units = declaredUnits(step);
      minimumUnits += units.minimum;
      maximumUnits += units.maximum;
    }
    if (autoAssembled) {
      providers.add("default provider");
      minimumUnits += 2;
      maximumUnits += 6;
    }
    if (config.use_orientation) providers.add("default provider");
    if (config.merge.enabled) {
      for (const step of steps.filter((candidate) => candidate.agents.length > 1)) {
        providers.add(config.merge.agents[0] || "default provider");
        minimumUnits += step.for_each ? 0 : 1;
        maximumUnits += step.for_each ? Math.max(step.for_each.max, 1) : 1;
      }
    }
    const perDocumentMinimum = minimumUnits + (config.use_orientation ? 1 : 0);
    const perDocumentMaximum = maximumUnits + (config.use_orientation ? 1 : 0);
    return {
      providers: [...providers],
      tools: [...tools],
      minimumUnits: perDocumentMinimum * batchCount,
      maximumUnits: perDocumentMaximum * batchCount,
    };
  }, [autoAssembled, batchCount, config.merge.agents, config.merge.enabled, config.use_orientation, steps]);
  const remoteIsPossible = summary.providers.includes("default provider")
    || summary.providers.some((provider) => provider !== "local");
  const workLabel = summary.minimumUnits === summary.maximumUnits
    ? `${summary.minimumUnits}`
    : `${summary.minimumUnits}–${summary.maximumUnits}`;
  const inputInterpretation = plan.inputInterpretation?.trim() || plan.inputMode || "input";

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-6" role="presentation">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="run-preview-title"
        tabIndex={-1}
        className="flex max-h-[88vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl bg-white shadow-2xl dark:bg-gray-900"
      >
        <header className="flex items-start gap-4 border-b border-gray-200 px-6 py-5 dark:border-gray-800">
          <div className="min-w-0 flex-1">
            <p className="text-xs font-semibold uppercase tracking-[0.12em] text-gray-500 dark:text-gray-400">Run preview</p>
            <h1 id="run-preview-title" className="mt-1 text-xl font-semibold text-gray-950 dark:text-gray-50">Review the execution plan</h1>
            <p className="mt-1 truncate text-sm text-gray-500 dark:text-gray-400">
              {batchCount > 1 ? `${batchCount} documents` : basename(inputPath)} · {inputInterpretation.replaceAll("_", " ")}
            </p>
          </div>
          <button type="button" aria-label="Close run preview" onClick={onCancel} className="rounded-lg p-1.5 text-gray-400 hover:bg-gray-100 hover:text-gray-800 dark:hover:bg-gray-800 dark:hover:text-gray-100">✕</button>
        </header>

        <div className="min-h-0 flex-1 overflow-auto px-6 py-5">
          <div className="grid gap-3 sm:grid-cols-4">
            <SummaryCard label="Documents" value={String(batchCount)} />
            <SummaryCard
              label="Enabled steps"
              value={autoAssembled ? `${steps.length} fixed` : conditionalSteps > 0
                ? `${steps.length - conditionalSteps} fixed + ${conditionalSteps} conditional`
                : String(steps.length)}
              detail={autoAssembled ? "+ 2–6 adaptive at run time" : undefined}
            />
            <SummaryCard label="Model work units" value={workLabel} />
            <SummaryCard label="Providers" value={summary.providers.join(", ") || "None"} />
          </div>

          <div className={`mt-5 rounded-xl border p-4 ${
            remoteIsPossible
              ? "border-amber-200 bg-amber-50 dark:border-amber-900 dark:bg-amber-950/25"
              : "border-emerald-200 bg-emerald-50 dark:border-emerald-900 dark:bg-emerald-950/25"
          }`}>
            <h2 className={`text-sm font-semibold ${remoteIsPossible ? "text-amber-900 dark:text-amber-200" : "text-emerald-900 dark:text-emerald-200"}`}>
              {remoteIsPossible ? "Selected artifacts may leave this computer" : "Explicit providers are local-only"}
            </h2>
            <p className={`mt-1 text-xs leading-5 ${remoteIsPossible ? "text-amber-800 dark:text-amber-300" : "text-emerald-800 dark:text-emerald-300"}`}>
              Each step receives only the artifacts listed below. Provider retries and schema-repair attempts are not included in the work-unit estimate.
            </p>
          </div>

          <section className="mt-6">
            <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">Execution timeline</h2>
            {autoAssembled && (
              <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                Orientation assembles 1–2 subject and 1–4 method specialists from the built-in catalog before execution.
              </p>
            )}
            <ol className="mt-3 grid gap-2 sm:grid-cols-2">
              {adaptiveTimelineStages.filter((stage) => stage.kind !== "done").map((stage, index) => (
                <li key={stage.id} className="flex items-start gap-3 rounded-xl border border-gray-200 px-3 py-2.5 dark:border-gray-800">
                  <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-gray-100 text-[11px] font-semibold text-gray-600 dark:bg-gray-800 dark:text-gray-300">{index + 1}</span>
                  <div className="min-w-0 flex-1">
                    <p className="text-xs font-medium text-gray-900 dark:text-gray-100">{stage.label}</p>
                    {stage.stepLabels && stage.stepLabels.length > 0 && (
                      <div className="mt-1.5 flex flex-wrap gap-1">
                        {stage.stepLabels.map((label, labelIndex) => (
                          <span
                            key={`${label}-${labelIndex}`}
                            className={`rounded-md px-1.5 py-0.5 text-[10px] leading-4 ${
                              autoAssembled && /specialists/.test(label)
                                ? "border border-dashed border-indigo-300 bg-indigo-50 text-indigo-700 dark:border-indigo-800 dark:bg-indigo-950/30 dark:text-indigo-300"
                                : "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300"
                            }`}
                          >
                            {label}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                </li>
              ))}
            </ol>
          </section>

          <section className="mt-6">
            <div className="flex items-baseline gap-3">
              <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">Artifact access by step</h2>
              {summary.tools.length > 0 && <span className="text-[11px] text-gray-500 dark:text-gray-400">Tools: {summary.tools.join(", ")}</span>}
            </div>
            <div className="mt-3 overflow-hidden rounded-xl border border-gray-200 dark:border-gray-800">
              {accessRows.map((row, index) => (
                  <div key={row.id} className={`grid gap-2 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)] ${index ? "border-t border-gray-200 dark:border-gray-800" : ""} ${row.adaptive ? "bg-indigo-50/40 dark:bg-indigo-950/10" : ""}`}>
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <p className="text-sm font-medium text-gray-900 dark:text-gray-100">{row.label}</p>
                        {row.adaptive && (
                          <span className="rounded-full bg-indigo-100 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-indigo-700 dark:bg-indigo-900/60 dark:text-indigo-200">
                            Auto-selected
                          </span>
                        )}
                      </div>
                      <p className="mt-0.5 text-[11px] text-gray-500 dark:text-gray-400">
                        {row.providers.join(", ")} · {row.unitLabel}
                        {row.conditional ? " · conditional" : ""}
                      </p>
                    </div>
                    <div className="flex flex-wrap items-center gap-1.5">
                      {row.selectors.length > 0 ? row.selectors.map((selector, selectorIndex) => (
                        <span key={`${selector.kind}-${selectorIndex}`} className="rounded-md bg-gray-100 px-2 py-1 text-[10px] text-gray-600 dark:bg-gray-800 dark:text-gray-300">{selectorLabel(selector)}</span>
                      )) : (
                        <span className="text-[11px] text-gray-400">No document artifacts</span>
                      )}
                      {row.additionalInputs?.map((input) => (
                        <span key={input} className="rounded-md bg-green-50 px-2 py-1 text-[10px] text-green-700 dark:bg-green-950/30 dark:text-green-300">{input}</span>
                      ))}
                      {row.tools.map((tool) => (
                        <span key={tool} className="rounded-md bg-blue-50 px-2 py-1 text-[10px] text-blue-700 dark:bg-blue-950/40 dark:text-blue-300">{tool}</span>
                      ))}
                    </div>
                  </div>
              ))}
            </div>
          </section>
        </div>

        <footer className="flex items-center justify-between border-t border-gray-200 px-6 py-4 dark:border-gray-800">
          <p className="text-[11px] text-gray-500 dark:text-gray-400">Extraction and actual fan-out determine final duration and usage.</p>
          <div className="flex gap-2">
            <button type="button" onClick={onCancel} className="rounded-lg border border-gray-300 px-4 py-2 text-sm text-gray-700 dark:border-gray-700 dark:text-gray-300">Back</button>
            <button data-autofocus type="button" onClick={onRun} className="rounded-lg bg-gray-900 px-4 py-2 text-sm font-medium text-white dark:bg-gray-100 dark:text-gray-900">Start run</button>
          </div>
        </footer>
      </div>
    </div>
  );
}

function SummaryCard({ label, value, detail }: { label: string; value: string; detail?: string }) {
  const title = detail ? `${value} ${detail}` : value;
  return (
    <div className="rounded-xl border border-gray-200 px-4 py-3 dark:border-gray-800">
      <p className="text-[11px] font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">{label}</p>
      <p className="mt-1 break-words text-base font-semibold leading-5 text-gray-950 dark:text-gray-50" title={title}>{value}</p>
      {detail && <p className="mt-0.5 text-[11px] leading-4 text-indigo-700 dark:text-indigo-300">{detail}</p>}
    </div>
  );
}
