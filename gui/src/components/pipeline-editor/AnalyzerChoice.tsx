import { useEffect, useState } from "react";
import { listAnalyzers, type AnalyzerInfo } from "../../lib/analyzers";

export interface AnalyzerSelection {
  info: AnalyzerInfo;
  network: boolean;
}

/** Network disclosure for an analyzer that can contact outside services. */
export function NetworkDisclosure({
  info,
  enabled,
  onChange,
}: {
  info: AnalyzerInfo;
  enabled: boolean;
  onChange: (enabled: boolean) => void;
}) {
  if (info.networkServices.length === 0) return null;
  const services = info.networkServices.join(" and ");
  return (
    <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 dark:border-amber-900 dark:bg-amber-950/25">
      <label className="flex items-start gap-2 text-xs text-amber-950 dark:text-amber-100">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(event) => onChange(event.target.checked)}
          className="mt-0.5"
        />
        <span>
          <span className="font-medium">Look up at {services}</span>
          <span className="mt-0.5 block text-[11px] leading-relaxed text-amber-800 dark:text-amber-300">
            Sends: {info.networkPayload} Without lookups, the check runs only
            local tests on this computer.
          </span>
        </span>
      </label>
    </div>
  );
}

/** Lists the Pipeline checks a workflow can run, for the Add Step dialog. */
export default function AnalyzerChoice({
  value,
  onChange,
}: {
  value: AnalyzerSelection | null;
  onChange: (selection: AnalyzerSelection) => void;
}) {
  const [analyzers, setAnalyzers] = useState<AnalyzerInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let stale = false;
    listAnalyzers()
      .then((items) => {
        if (!stale) setAnalyzers(items);
      })
      .catch((caught) => {
        if (!stale)
          setError(caught instanceof Error ? caught.message : String(caught));
      });
    return () => {
      stale = true;
    };
  }, []);

  if (error) {
    return (
      <p role="alert" className="text-xs text-red-600 dark:text-red-400">
        Could not load Pipeline checks: {error}
      </p>
    );
  }
  if (!analyzers) {
    return <p className="text-xs text-gray-500">Loading checks…</p>;
  }
  return (
    <div className="space-y-3">
      <p className="text-xs leading-5 text-gray-600 dark:text-gray-400">
        Pipeline checks run on this computer and make no model call. Their
        results can feed later steps or serve as this Review&apos;s findings.
      </p>
      <div role="radiogroup" aria-label="Pipeline check" className="space-y-2">
        {analyzers.map((info) => {
          const checked =
            value?.info.id === info.id && value.info.version === info.version;
          return (
            <label
              key={`${info.id}@${info.version}`}
              className={`flex cursor-pointer items-start gap-2.5 rounded-lg border p-3 ${
                checked
                  ? "border-gray-700 bg-gray-50 dark:border-gray-300 dark:bg-gray-800"
                  : "border-gray-200 hover:border-gray-400 dark:border-gray-700"
              }`}
            >
              <input
                type="radio"
                name="pipeline-check"
                checked={checked}
                onChange={() =>
                  onChange({ info, network: value?.network ?? false })
                }
                className="mt-0.5"
              />
              <span>
                <span className="block text-xs font-medium text-gray-800 dark:text-gray-200">
                  {info.label}
                </span>
                <span className="mt-0.5 block text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">
                  {info.description}
                </span>
                <span className="mt-1 block text-[11px] text-gray-500 dark:text-gray-400">
                  Reads: {info.reads.join("; ")}
                </span>
              </span>
            </label>
          );
        })}
      </div>
      {value && (
        <NetworkDisclosure
          info={value.info}
          enabled={value.network}
          onChange={(network) => onChange({ ...value, network })}
        />
      )}
    </div>
  );
}
