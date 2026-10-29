import {
  formatRunDate,
  formatRunDuration,
  formatTokens,
  type RunProvenance,
} from "../../lib/runProvenance";
import type { ReportQuality } from "../../lib/types";

export default function ProvenancePanel({
  provenance,
  quality,
}: {
  provenance: RunProvenance;
  quality?: ReportQuality;
}) {
  const fields = [
    ["Review subject", provenance.subject],
    ["Authors", provenance.authors.join(", ") || "—"],
    ["Workflow", provenance.workflow],
    ["Completed", formatRunDate(provenance.completed)],
    ["Duration", formatRunDuration(provenance.duration_secs)],
    ["Report", provenance.run_id || "Unsaved"],
    ["Provider", provenance.provider_summary],
    ["Model", provenance.model_summary],
  ];
  const showCacheWrite = provenance.totals.cache_write_input_tokens > 0;
  const status = quality?.status || provenance.status;

  return (
    <div className="h-full overflow-y-auto">
      <section
        aria-label="Report details"
        className="mx-auto max-w-5xl px-8 py-10"
      >
        <div className="flex items-start justify-between gap-6">
          <div>
            <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-gray-500 dark:text-gray-400">
              Completed report
            </p>
            <h2 className="mt-1 text-2xl font-semibold tracking-[-0.025em] text-gray-950 dark:text-gray-50">
              Report details
            </h2>
            <p className="mt-2 max-w-2xl text-sm text-gray-600 dark:text-gray-300">
              See the models, settings, and reported token usage for this
              review.
            </p>
          </div>
          <span
            className={`rounded-full px-2.5 py-1 text-xs font-medium ${
              status === "done"
                ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/50 dark:text-emerald-300"
                : "bg-amber-50 text-amber-700 dark:bg-amber-950/50 dark:text-amber-300"
            }`}
          >
            {status === "done" ? "Completed" : status}
          </span>
        </div>

        <dl className="mt-8 grid gap-px overflow-hidden rounded-xl border border-gray-200 bg-gray-200 sm:grid-cols-2 lg:grid-cols-4 dark:border-gray-800 dark:bg-gray-800">
          {fields.map(([label, value]) => (
            <div
              key={label}
              className="min-w-0 bg-white px-4 py-3.5 dark:bg-gray-950"
            >
              <dt className="text-[11px] font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                {label}
              </dt>
              <dd
                className="mt-1 break-words text-sm font-medium text-gray-800 dark:text-gray-200"
                title={value}
              >
                {value}
              </dd>
            </div>
          ))}
        </dl>

        {quality && quality.schema_version > 0 && (
          <section aria-labelledby="quality-ledger-heading" className="mt-10">
            <h3
              id="quality-ledger-heading"
              className="text-base font-semibold text-gray-950 dark:text-gray-50"
            >
              Quality and limitations
            </h3>
            <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
              Pipeline checks which material was reviewed and which findings
              have supporting evidence.
            </p>
            <dl className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
              {[
                [
                  "Extraction",
                  `${quality.recovered_pages} pages · ${quality.recovered_blocks} blocks · ${quality.recovered_assets} assets`,
                ],
                [
                  "Orientation",
                  `${quality.orientation_included_bytes.toLocaleString()} included · ${quality.orientation_omitted_bytes.toLocaleString()} omitted bytes`,
                ],
                [
                  "Reviewers",
                  `${quality.completed_steps?.length ?? 0} completed · ${quality.failed_steps?.length ?? 0} failed · ${quality.skipped_steps?.length ?? 0} skipped`,
                ],
                [
                  "Evidence",
                  `${quality.verified_evidence} verified · ${quality.partially_verified_evidence} partial · ${quality.unverified_evidence} unverified`,
                ],
              ].map(([label, value]) => (
                <div
                  key={label}
                  className="rounded-xl border border-gray-200 p-4 dark:border-gray-800"
                >
                  <dt className="text-[10px] font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">
                    {label}
                  </dt>
                  <dd className="mt-1 text-sm text-gray-800 dark:text-gray-200">
                    {value}
                  </dd>
                </div>
              ))}
            </dl>
            {(quality.limitations?.length ?? 0) > 0 ? (
              <div className="mt-4 rounded-xl border border-amber-200 bg-amber-50/70 p-4 text-sm text-amber-900 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-200">
                <p className="font-semibold">Recorded limitations</p>
                <ul className="mt-2 list-disc space-y-1 pl-5">
                  {quality.limitations?.map((limitation, index) => (
                    <li key={`${index}-${limitation}`}>{limitation}</li>
                  ))}
                </ul>
              </div>
            ) : (
              <p className="mt-4 text-sm text-emerald-700 dark:text-emerald-300">
                No limitations were reported by the automatic checks.
              </p>
            )}
            <p className="mt-3 break-all font-mono text-[11px] text-gray-500 dark:text-gray-400">
              Reproducibility hash:{" "}
              {quality.reproducibility_hash || "Unavailable"}
            </p>
          </section>
        )}

        <div className="mt-10">
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div>
              <h3 className="text-base font-semibold text-gray-950 dark:text-gray-50">
                Token usage
              </h3>
              <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                Cached input is a subset of logical input, as reported by each
                provider.
              </p>
            </div>
            <div className="flex gap-5 text-right">
              {[
                ["Input", provenance.totals.input_tokens],
                ["Output", provenance.totals.output_tokens],
                ["Cached", provenance.totals.cached_input_tokens],
              ].map(([label, value]) => (
                <div key={label}>
                  <p className="text-[10px] font-medium uppercase tracking-wide text-gray-500 dark:text-gray-400">
                    {label}
                  </p>
                  <p className="text-sm font-semibold tabular-nums text-gray-900 dark:text-gray-100">
                    {formatTokens(value as number)}
                  </p>
                </div>
              ))}
            </div>
          </div>

          <div className="mt-4 overflow-x-auto rounded-xl border border-gray-200 dark:border-gray-800">
            <table className="w-full min-w-[52rem] border-collapse text-left text-sm">
              <thead className="bg-gray-50 text-[11px] uppercase tracking-wide text-gray-500 dark:bg-gray-900 dark:text-gray-400">
                <tr>
                  <th className="px-4 py-2.5 font-medium">Step</th>
                  <th className="px-4 py-2.5 font-medium">Provider</th>
                  <th className="px-4 py-2.5 font-medium">Model</th>
                  <th className="px-4 py-2.5 text-right font-medium">Input</th>
                  <th className="px-4 py-2.5 text-right font-medium">Output</th>
                  <th className="px-4 py-2.5 text-right font-medium">Cached</th>
                  {showCacheWrite && (
                    <th className="px-4 py-2.5 text-right font-medium">
                      Cache write
                    </th>
                  )}
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100 dark:divide-gray-800">
                {provenance.step_usage.length ? (
                  provenance.step_usage.map((row) => (
                    <tr key={`${row.step_id}:${row.provider}:${row.model}`}>
                      <td
                        className="px-4 py-3 font-medium text-gray-800 dark:text-gray-200"
                        title={row.step_id}
                      >
                        {row.step_label}
                      </td>
                      <td className="px-4 py-3 font-medium text-gray-800 dark:text-gray-200">
                        {row.provider}
                        {row.transports.length > 0 && (
                          <span className="ml-2 text-[10px] font-semibold text-gray-400">
                            {row.transports.join(" + ")}
                          </span>
                        )}
                      </td>
                      <td className="px-4 py-3 text-gray-600 dark:text-gray-300">
                        {row.model}
                      </td>
                      <td className="px-4 py-3 text-right tabular-nums text-gray-600 dark:text-gray-300">
                        {formatTokens(row.input_tokens)}
                      </td>
                      <td className="px-4 py-3 text-right tabular-nums text-gray-600 dark:text-gray-300">
                        {formatTokens(row.output_tokens)}
                      </td>
                      <td className="px-4 py-3 text-right tabular-nums text-gray-600 dark:text-gray-300">
                        {formatTokens(row.cached_input_tokens)}
                      </td>
                      {showCacheWrite && (
                        <td className="px-4 py-3 text-right tabular-nums text-gray-600 dark:text-gray-300">
                          {formatTokens(row.cache_write_input_tokens)}
                        </td>
                      )}
                    </tr>
                  ))
                ) : (
                  <tr>
                    <td
                      colSpan={showCacheWrite ? 7 : 6}
                      className="px-4 py-5 text-center text-gray-500 dark:text-gray-400"
                    >
                      This provider did not report token usage.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
          {!provenance.usage_matches_total && (
            <p className="mt-3 text-xs text-gray-500 dark:text-gray-400">
              Totals may include document reading, input assessment, setup
              calls, and reused results that cannot be assigned to a model in
              this table.
            </p>
          )}
        </div>
      </section>
    </div>
  );
}
