import { useMemo } from "react";
import type { PipelineConfig, StepConfig } from "../lib/types";
import type { ExecutionPlanStage } from "../lib/pipelineHelpers";
import useModalDialog from "../hooks/useModalDialog";
import {
  adaptiveAgentCount as getAdaptiveAgentCount,
  adaptiveAgentCountLabel,
  adaptiveAgentRange as getAdaptiveAgentRange,
  isAutoReview,
} from "../lib/autoReview";
import { PROVIDER_LABELS, type Provider } from "../lib/providers";
import RunUsageEstimate, { SUMMARY_NOTE, SummaryRow } from "./RunUsageEstimate";
import type { UsageEstimate } from "../lib/usageEstimate";
import type { HostCheck } from "../lib/analyzers";

export interface RunPreviewPlan {
  profileId: string;
  inputMode: string;
  inputInterpretation: string;
  stages: ExecutionPlanStage[];
  /** Every provider the run may call, as the readiness check counts them. */
  providers?: string[];
  orientationAgent?: string;
  /** The usage-limit backup, when it couldn't run right now. */
  backupUnavailable?: string | null;
  usageEstimate?: UsageEstimate;
  hostChecks?: HostCheck[];
}

interface Props {
  plan: RunPreviewPlan;
  config: PipelineConfig;
  inputPath: string;
  batchCount?: number;
  /** Every review slot is busy, so this one waits in the queue. */
  queued?: boolean;
  onCancel: () => void;
  onRun: () => void;
}

function basename(path: string): string {
  const parts = path.replaceAll("\\", "/").split("/").filter(Boolean);
  return parts[parts.length - 1] || "No input";
}

function providerList(providers: string[]): string {
  const list = providers
    .map((provider) => PROVIDER_LABELS[provider as Provider] ?? provider)
    .join(", ");
  return list.charAt(0).toUpperCase() + list.slice(1);
}

function selectorLabel(
  selector: StepConfig["context"]["include"][number],
  stepLabels: Map<string, string>,
): string {
  switch (selector.kind) {
    case "primary":
      return `Document ${selector.parts.join(", ")}`;
    case "survey":
      return "Orientation map";
    case "named_input":
      return `Input: ${selector.key}`;
    case "step":
      return `${stepLabels.get(selector.step) ?? selector.step} results`;
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

interface AccessRow {
  id: string;
  label: string;
  providers: string[];
  unitLabel: string;
  reads: string[];
  tools: string[];
  conditional?: boolean;
}

function fixedAccessRow(
  step: StepConfig,
  stepLabels: Map<string, string>,
): AccessRow {
  if (step.kind === "analyzer") {
    return {
      id: step.id,
      label: step.label,
      providers: ["Pipeline check (no model call)"],
      unitLabel: "runs on this computer",
      reads: [],
      tools: [],
      conditional: !!step.run_if,
    };
  }
  const units = declaredUnits(step);
  const reads = step.context.include.map((selector) =>
    selectorLabel(selector, stepLabels),
  );
  return {
    id: step.id,
    label: step.label,
    providers: step.agents,
    unitLabel:
      units.maximum > units.minimum
        ? `up to ${units.maximum} calls`
        : `${units.minimum} call${units.minimum === 1 ? "" : "s"}`,
    reads: reads.length > 0 ? reads : ["Nothing from the document"],
    tools: step.tools,
    conditional: !!step.run_if,
  };
}

export default function RunPreview({
  plan,
  config,
  inputPath,
  batchCount = 1,
  queued = false,
  onCancel,
  onRun,
}: Props) {
  const dialogRef = useModalDialog<HTMLDivElement>(onCancel);
  const steps = config.steps.filter((step) => step.enabled);
  const conditionalSteps = steps.filter((step) => !!step.run_if).length;
  const autoAssembled = isAutoReview(config);
  const adaptiveCount = getAdaptiveAgentCount(config);
  const adaptiveRange = getAdaptiveAgentRange(config);
  const adaptiveCountLabel = adaptiveAgentCountLabel(
    adaptiveCount,
    adaptiveRange,
  );
  const accessRows = useMemo(() => {
    const stepLabels = new Map(
      config.steps.map((step) => [step.id, step.label]),
    );
    const rows: AccessRow[] = [];
    if (config.use_orientation) {
      rows.push({
        id: "orientation",
        label: "Orientation",
        providers: plan.orientationAgent ? [plan.orientationAgent] : [],
        unitLabel: "1 call",
        reads: ["The document"],
        tools: [],
      });
    }
    // Specialists copy the first review step's providers and access.
    const specialistDonor = steps.find(
      (step) => step.phase === "parallel" && step.kind !== "analyzer",
    );
    let inserted = false;
    const insertAdaptiveRows = () => {
      if (!autoAssembled || inserted) return;
      inserted = true;
      rows.push({
        id: "auto_adaptive_agents",
        label: "Specialists",
        providers: specialistDonor?.agents ?? [],
        unitLabel: `${adaptiveCountLabel} calls`,
        reads: (specialistDonor?.context.include ?? []).map((selector) =>
          selectorLabel(selector, stepLabels),
        ),
        tools: [],
      });
    };
    for (const step of steps) {
      if (step.id === "auto_synthesis" || step.phase === "sequential")
        insertAdaptiveRows();
      const row = fixedAccessRow(step, stepLabels);
      if (autoAssembled && step.id === "auto_synthesis") {
        row.reads.push(`Specialist results (${adaptiveCountLabel})`);
      }
      rows.push(row);
    }
    insertAdaptiveRows();
    return rows;
  }, [
    adaptiveCountLabel,
    autoAssembled,
    config.steps,
    config.use_orientation,
    plan.orientationAgent,
    steps,
  ]);
  const summary = useMemo(() => {
    let minimumUnits = 0;
    let maximumUnits = 0;
    for (const step of steps) {
      // Pipeline checks run on this computer and make no provider call.
      if (step.kind === "analyzer") continue;
      const units = declaredUnits(step);
      minimumUnits += units.minimum;
      maximumUnits += units.maximum;
    }
    if (autoAssembled) {
      minimumUnits += adaptiveCount ?? adaptiveRange.totalMin;
      maximumUnits += adaptiveCount ?? adaptiveRange.totalMax;
    }
    if (config.merge.enabled) {
      for (const step of steps.filter(
        (candidate) => candidate.agents.length > 1,
      )) {
        minimumUnits += step.for_each ? 0 : 1;
        maximumUnits += step.for_each ? Math.max(step.for_each.max, 1) : 1;
      }
    }
    const perDocumentMinimum = minimumUnits + 1;
    const perDocumentMaximum = maximumUnits + 1;
    return {
      minimumUnits: perDocumentMinimum * batchCount,
      maximumUnits: perDocumentMaximum * batchCount,
    };
  }, [
    adaptiveCount,
    adaptiveRange.totalMax,
    adaptiveRange.totalMin,
    autoAssembled,
    batchCount,
    config.merge.enabled,
    steps,
  ]);
  const workLabel =
    summary.minimumUnits === summary.maximumUnits
      ? `${summary.minimumUnits}`
      : `${summary.minimumUnits}–${summary.maximumUnits}`;
  const inputInterpretation =
    plan.inputInterpretation?.trim() || plan.inputMode || "input";
  const networkChecks = (plan.hostChecks ?? []).filter(
    (check) => check.networkServices.length > 0,
  );
  const stepsLabel = (count: number) =>
    `${count} step${count === 1 ? "" : "s"}`;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-6"
      role="presentation"
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="run-preview-title"
        tabIndex={-1}
        className="flex max-h-[88vh] w-full max-w-xl flex-col overflow-hidden rounded-2xl bg-white shadow-2xl dark:bg-gray-900"
      >
        <div className="min-h-0 flex-1 overflow-auto px-6 pb-2 pt-6">
          <h1
            id="run-preview-title"
            className="text-xl font-semibold text-gray-950 dark:text-gray-50"
          >
            Ready to start
          </h1>
          <p className="mt-1 truncate text-sm text-gray-500 dark:text-gray-400">
            {batchCount > 1 ? `${batchCount} documents` : basename(inputPath)} ·{" "}
            {inputInterpretation.replaceAll("_", " ")}
          </p>

          <dl className="mt-6 space-y-4">
            <SummaryRow label="Steps">
              {autoAssembled
                ? `${stepsLabel(steps.length)} + ${adaptiveCountLabel} specialists chosen for each document`
                : conditionalSteps > 0
                  ? `${stepsLabel(steps.length - conditionalSteps)} + ${conditionalSteps} conditional`
                  : stepsLabel(steps.length)}
            </SummaryRow>
            <SummaryRow label="Providers">
              {providerList(plan.providers ?? [])}
              <p className={SUMMARY_NOTE}>
                See below for what each step can send.
              </p>
              {plan.backupUnavailable && (
                <p className="mt-1 text-xs leading-5 text-amber-700 dark:text-amber-300">
                  Your backup, {providerList([plan.backupUnavailable])}, isn’t
                  connected. If you reach a usage limit, this review stops.
                </p>
              )}
            </SummaryRow>
            {plan.usageEstimate ? (
              <RunUsageEstimate
                estimate={plan.usageEstimate}
                batchCount={batchCount}
              />
            ) : (
              <SummaryRow label="Estimated use">
                {workLabel} model calls
              </SummaryRow>
            )}
          </dl>

          {networkChecks.length > 0 && (
            <section
              aria-label="Pipeline checks that use the network"
              className="mt-5 rounded-xl border border-amber-200 bg-amber-50 p-4 dark:border-amber-900 dark:bg-amber-950/25"
            >
              <h2 className="text-sm font-semibold text-amber-900 dark:text-amber-200">
                Lookups outside your AI provider
              </h2>
              <ul className="mt-1 space-y-1 text-xs leading-5 text-amber-800 dark:text-amber-300">
                {networkChecks.map((check) => (
                  <li key={check.stepId}>
                    <span className="font-medium">{check.stepLabel}</span>{" "}
                    contacts {check.networkServices.join(" and ")}. Sends:{" "}
                    {check.networkPayload}
                  </li>
                ))}
              </ul>
            </section>
          )}

          <details className="mt-6 border-t border-gray-200 py-3 dark:border-gray-800">
            <summary className="cursor-pointer text-sm font-medium text-gray-700 dark:text-gray-300">
              Steps and what each can read
            </summary>
            <ul className="mt-2 divide-y divide-gray-100 dark:divide-gray-800">
              {accessRows.map((row) => (
                <li key={row.id} className="py-2.5">
                  <div className="flex flex-wrap items-baseline justify-between gap-x-4">
                    <p className="text-sm font-medium text-gray-900 dark:text-gray-100">
                      {row.label}
                    </p>
                    <p className="text-xs text-gray-500 dark:text-gray-400">
                      {providerList(row.providers)} · {row.unitLabel}
                      {row.conditional ? " · conditional" : ""}
                    </p>
                  </div>
                  {row.reads.length > 0 && (
                    <p className={SUMMARY_NOTE}>
                      Reads: {row.reads.join(" · ")}
                    </p>
                  )}
                  {row.tools.length > 0 && (
                    <p className={SUMMARY_NOTE}>
                      Tools: {row.tools.join(", ")}
                    </p>
                  )}
                </li>
              ))}
            </ul>
          </details>
        </div>

        <footer className="flex justify-end gap-2 px-6 py-4">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-lg border border-gray-300 px-4 py-2 text-sm text-gray-700 dark:border-gray-700 dark:text-gray-300"
          >
            Back
          </button>
          <button
            data-autofocus
            type="button"
            onClick={onRun}
            className="rounded-lg bg-gray-900 px-4 py-2 text-sm font-medium text-white dark:bg-gray-100 dark:text-gray-900"
          >
            {batchCount > 1
              ? `Queue ${batchCount} reviews`
              : queued
                ? "Add to queue"
                : "Generate report"}
          </button>
        </footer>
      </div>
    </div>
  );
}
