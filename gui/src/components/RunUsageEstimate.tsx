import {
  formatTokens,
  formatUsd,
  limitSummary,
  type UsageEstimate,
} from "../lib/usageEstimate";

function range(low: number, high: number, format: (value: number) => string) {
  return low === high ? format(low) : `${format(low)}–${format(high)}`;
}

export default function RunUsageEstimate({
  estimate,
  batchCount = 1,
}: {
  estimate?: UsageEstimate;
  batchCount?: number;
}) {
  if (!estimate) return null;
  const limits = limitSummary(estimate);
  const hasApiCost =
    estimate.apiCostLow != null && estimate.apiCostHigh != null;
  const overLimit = estimate.exceedsCostLimit || estimate.exceedsCallLimit;
  return (
    <section
      aria-label="Estimated use"
      className="mt-5 rounded-xl border border-gray-200 p-4 dark:border-gray-800"
    >
      <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">
        Estimated use{batchCount > 1 ? " per document" : ""}
      </h2>
      <dl className="mt-2 grid gap-x-6 gap-y-1 text-xs text-gray-700 sm:grid-cols-2 dark:text-gray-300">
        <div className="flex justify-between gap-3">
          <dt className="text-gray-500 dark:text-gray-400">Model calls</dt>
          <dd>{range(estimate.callsLow, estimate.callsHigh, String)}</dd>
        </div>
        <div className="flex justify-between gap-3">
          <dt className="text-gray-500 dark:text-gray-400">Tokens in · out</dt>
          <dd>
            {range(
              estimate.inputTokensLow,
              estimate.inputTokensHigh,
              formatTokens,
            )}{" "}
            ·{" "}
            {range(
              estimate.outputTokensLow,
              estimate.outputTokensHigh,
              formatTokens,
            )}
          </dd>
        </div>
        {hasApiCost && (
          <div className="flex justify-between gap-3">
            <dt className="text-gray-500 dark:text-gray-400">API list price</dt>
            <dd>
              {range(estimate.apiCostLow!, estimate.apiCostHigh!, formatUsd)}
            </dd>
          </div>
        )}
        <div className="flex justify-between gap-3">
          <dt className="text-gray-500 dark:text-gray-400">Limits</dt>
          <dd>{limits ?? "None set"}</dd>
        </div>
      </dl>
      <p className="mt-2 text-[11px] leading-5 text-gray-500 dark:text-gray-400">
        A rough range from the input&apos;s size and the planned steps. Retries,
        tool reads, and the specialists orientation selects change actual use.
        {estimate.subscriptionCallsHigh > 0 &&
          " Subscription calls count against your plan's limits and are not billed per token."}
        {estimate.unpricedCalls > 0 &&
          ` ${estimate.unpricedCalls} API call${estimate.unpricedCalls === 1 ? " uses" : "s use"} a model without a known price.`}
        {!limits &&
          " You can set a cost, call, or time limit in Settings → Reviews."}
      </p>
      {overLimit && (
        <p
          role="status"
          className="mt-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:border-amber-900 dark:bg-amber-950/25 dark:text-amber-200"
        >
          The high end of this estimate is above your limit. If the limit is
          reached, steps not yet started are skipped and the completed results
          are saved as a partial report.
        </p>
      )}
    </section>
  );
}
