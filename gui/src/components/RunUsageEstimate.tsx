import type { ReactNode } from "react";
import {
  formatTokens,
  formatUsd,
  limitSummary,
  type UsageEstimate,
} from "../lib/usageEstimate";

export const SUMMARY_NOTE =
  "mt-0.5 text-xs leading-5 text-gray-500 dark:text-gray-400";

/** One labelled line in the run preview's summary list. */
export function SummaryRow({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="grid gap-x-4 sm:grid-cols-[7rem_minmax(0,1fr)]">
      <dt className="text-sm text-gray-500 dark:text-gray-400">{label}</dt>
      <dd className="min-w-0 text-sm text-gray-900 dark:text-gray-100">
        {children}
      </dd>
    </div>
  );
}

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
    <>
      <SummaryRow label="Estimated use">
        {range(estimate.callsLow, estimate.callsHigh, String)} model{" "}
        {estimate.callsHigh === 1 ? "call" : "calls"}
        {batchCount > 1 ? " per document" : ""}
        {hasApiCost &&
          ` · ${range(estimate.apiCostLow!, estimate.apiCostHigh!, formatUsd)} at API list price`}
        <p className={SUMMARY_NOTE}>
          {range(
            estimate.inputTokensLow,
            estimate.inputTokensHigh,
            formatTokens,
          )}{" "}
          tokens in ·{" "}
          {range(
            estimate.outputTokensLow,
            estimate.outputTokensHigh,
            formatTokens,
          )}{" "}
          out. Retries, tool reads, and the specialists chosen change actual
          use.
          {estimate.subscriptionCallsHigh > 0 &&
            " Subscription calls count against your plan's limits and are not billed per token."}
          {estimate.unpricedCalls > 0 &&
            ` ${estimate.unpricedCalls} API call${estimate.unpricedCalls === 1 ? " uses" : "s use"} a model without a known price.`}
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
      </SummaryRow>
      {limits && <SummaryRow label="Limits">{limits}</SummaryRow>}
    </>
  );
}
