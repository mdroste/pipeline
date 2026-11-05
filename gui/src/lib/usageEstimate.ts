/** Rough preflight range computed by the host for one Review. */
export interface UsageEstimate {
  documentTokens: number;
  callsLow: number;
  callsHigh: number;
  inputTokensLow: number;
  inputTokensHigh: number;
  outputTokensLow: number;
  outputTokensHigh: number;
  apiCostLow?: number | null;
  apiCostHigh?: number | null;
  unpricedCalls: number;
  subscriptionCallsHigh: number;
  maxCostUsd?: number | null;
  maxCalls?: number | null;
  maxMinutes?: number | null;
  exceedsCostLimit: boolean;
  exceedsCallLimit: boolean;
}

export function formatTokens(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${Math.round(value / 1_000)}k`;
  return String(value);
}

export function formatUsd(value: number): string {
  return value >= 1 ? `$${value.toFixed(2)}` : `$${value.toFixed(3)}`;
}

export function limitSummary(estimate: UsageEstimate): string | null {
  const parts: string[] = [];
  if (estimate.maxCostUsd) parts.push(`${formatUsd(estimate.maxCostUsd)}`);
  if (estimate.maxCalls) parts.push(`${estimate.maxCalls} model calls`);
  if (estimate.maxMinutes) parts.push(`${estimate.maxMinutes} minutes`);
  return parts.length > 0 ? parts.join(" · ") : null;
}
