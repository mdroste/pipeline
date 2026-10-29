import { useEffect, useState } from "react";
import type { Phase, PipelineConfig, StepConfig } from "../../lib/types";
import {
  analyzerNetworkEnabled,
  listAnalyzers,
  type AnalyzerInfo,
} from "../../lib/analyzers";
import { NetworkDisclosure } from "./AnalyzerChoice";

/** Editor for a Pipeline check step: no prompt, providers, or model. */
export default function AnalyzerStepPanel({
  step,
  config,
  onUpdate,
  onPhaseChange,
  onOutputRoleChange,
}: {
  step: StepConfig;
  config: PipelineConfig;
  onUpdate: (id: string, patch: Partial<StepConfig>) => void;
  onPhaseChange: (id: string, phase: Phase) => void;
  onOutputRoleChange: (
    id: string,
    role: "primary_step" | "findings_step",
    enabled: boolean,
  ) => void;
}) {
  const [info, setInfo] = useState<AnalyzerInfo | null | undefined>(undefined);
  const spec = step.analyzer;
  useEffect(() => {
    let stale = false;
    listAnalyzers()
      .then((items) => {
        if (stale) return;
        setInfo(
          items.find(
            (item) => item.id === spec?.id && item.version === spec?.version,
          ) ?? null,
        );
      })
      .catch(() => {
        if (!stale) setInfo(null);
      });
    return () => {
      stale = true;
    };
  }, [spec?.id, spec?.version]);

  const publishesFindings = config.outputs?.findings_step === step.id;
  return (
    <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
      <div>
        <label className="block text-xs font-medium text-gray-500 mb-1">
          Step name
        </label>
        <input
          type="text"
          aria-label="Step label"
          value={step.label}
          onChange={(event) => onUpdate(step.id, { label: event.target.value })}
          className="w-full py-1.5 px-3 border border-gray-300 dark:border-gray-600 rounded-lg text-sm font-medium text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200"
        />
      </div>
      <section className="rounded-lg border border-gray-200 p-3 dark:border-gray-700">
        <h3 className="text-xs font-semibold text-gray-900 dark:text-gray-100">
          Pipeline check
          {spec ? ` · ${spec.id} v${spec.version}` : ""}
        </h3>
        {info === undefined ? (
          <p className="mt-1 text-xs text-gray-500">Loading…</p>
        ) : info === null ? (
          <p
            role="alert"
            className="mt-1 text-xs text-red-600 dark:text-red-400"
          >
            This version of Pipeline does not provide this check. The workflow
            cannot run until the step is removed.
          </p>
        ) : (
          <>
            <p className="mt-1 text-xs leading-5 text-gray-600 dark:text-gray-400">
              {info.description}
            </p>
            <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">
              Reads: {info.reads.join("; ")}. Runs on this computer and makes no
              model call.
            </p>
          </>
        )}
      </section>
      {info && spec && (
        <NetworkDisclosure
          info={info}
          enabled={analyzerNetworkEnabled(step)}
          onChange={(network) =>
            onUpdate(step.id, {
              analyzer: {
                ...spec,
                options: { ...(spec.options ?? {}), network },
              },
            })
          }
        />
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="flex flex-col gap-1 text-xs text-gray-600 dark:text-gray-400">
          When it runs
          <select
            aria-label="Step phase"
            value={step.phase}
            onChange={(event) =>
              onPhaseChange(step.id, event.target.value as Phase)
            }
            className="rounded-lg border border-gray-300 bg-white px-2 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-800"
          >
            <option value="parallel">With the parallel reviewers</option>
            <option value="sequential">After earlier steps, on its own</option>
          </select>
        </label>
        <label className="flex items-center gap-2 self-end text-xs text-gray-700 dark:text-gray-300">
          <input
            type="checkbox"
            checked={publishesFindings}
            disabled={step.phase !== "sequential" && !publishesFindings}
            onChange={(event) =>
              onOutputRoleChange(step.id, "findings_step", event.target.checked)
            }
          />
          <span>
            Use this check&apos;s results as the Review&apos;s findings
            {step.phase !== "sequential" && (
              <span className="block text-[11px] text-gray-500">
                Available when it runs after earlier steps.
              </span>
            )}
          </span>
        </label>
      </div>
      <p className="text-[11px] leading-5 text-gray-500 dark:text-gray-400">
        Later model steps can read this check&apos;s results by selecting this
        step&apos;s output under Inputs.
      </p>
    </div>
  );
}
