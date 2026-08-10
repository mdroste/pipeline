import {
  isPaperOrientation,
  type PipelineReport,
  type RunSummary,
  type StepCallRecord,
  type StepOutput,
} from "./types";

export interface UsageTotals {
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens: number;
  cache_write_input_tokens: number;
}

export interface ProviderModelUsage extends UsageTotals {
  provider: string;
  model: string;
  transports: string[];
  efforts: string[];
}

export interface StepModelUsage extends ProviderModelUsage {
  step_id: string;
  step_label: string;
}

export interface ProvenanceManifest {
  run_id: string;
  created?: string;
  input_path?: string;
  profile_id?: string;
  profile_name?: string;
  provider?: string;
  status?: string;
  duration_secs?: number;
  title?: string;
  usage?: Partial<UsageTotals>;
}

export interface RunProvenance {
  subject: string;
  authors: string[];
  workflow: string;
  completed: string;
  duration_secs: number | null;
  run_id: string;
  status: string;
  usage: ProviderModelUsage[];
  step_usage: StepModelUsage[];
  totals: UsageTotals;
  usage_matches_total: boolean;
  provider_summary: string;
  model_summary: string;
}

const ZERO_USAGE: UsageTotals = {
  input_tokens: 0,
  output_tokens: 0,
  cached_input_tokens: 0,
  cache_write_input_tokens: 0,
};

function count(value: number | undefined): number {
  return Number.isFinite(value) && value! > 0 ? Math.round(value!) : 0;
}

function addUsage(target: UsageTotals, source: Partial<UsageTotals>) {
  target.input_tokens += count(source.input_tokens);
  target.output_tokens += count(source.output_tokens);
  target.cached_input_tokens += count(source.cached_input_tokens);
  target.cache_write_input_tokens += count(source.cache_write_input_tokens);
}

function hasUsage(usage: UsageTotals): boolean {
  return Object.values(usage).some((value) => value > 0);
}

function displayProvider(value: string): string {
  const normalized = value.trim();
  if (!normalized) return "Default";
  if (normalized.toLowerCase() === "openai") return "OpenAI";
  return normalized[0].toUpperCase() + normalized.slice(1);
}

function usageIdentity(
  step: StepOutput,
  call?: StepCallRecord,
): { provider: string; model: string; transport: string; effort: string } {
  const provider =
    call?.provider?.trim() ||
    call?.agent?.trim() ||
    step.provider?.trim() ||
    step.agent?.trim() ||
    "default";
  return {
    provider: displayProvider(provider),
    model: call?.model?.trim() || step.model?.trim() || "Automatic",
    transport: call?.model_transport?.trim() || step.model_transport?.trim() || "",
    effort: call?.effort?.trim() || "",
  };
}

/**
 * Attribute saved token records to provider/model pairs. Per-call records are
 * authoritative when present because merged multi-agent outputs contain calls
 * from every source provider as well as the merge provider.
 */
export function aggregateProviderModelUsage(
  report: PipelineReport,
): ProviderModelUsage[] {
  const groups = new Map<
    string,
    ProviderModelUsage & { transportSet: Set<string>; effortSet: Set<string> }
  >();

  for (const step of report.step_outputs) {
    const records: Array<StepCallRecord | undefined> = step.calls?.length
      ? step.calls
      : [undefined];
    for (const call of records) {
      const identity = usageIdentity(step, call);
      const key = `${identity.provider.toLocaleLowerCase()}\u0000${identity.model.toLocaleLowerCase()}`;
      let group = groups.get(key);
      if (!group) {
        group = {
          provider: identity.provider,
          model: identity.model,
          transports: [],
          efforts: [],
          transportSet: new Set<string>(),
          effortSet: new Set<string>(),
          ...ZERO_USAGE,
        };
        groups.set(key, group);
      }
      addUsage(group, call ?? step);
      if (identity.transport) group.transportSet.add(identity.transport.toUpperCase());
      if (identity.effort) group.effortSet.add(identity.effort);
    }
  }

  return Array.from(groups.values()).map(({ transportSet, effortSet, ...group }) => ({
    ...group,
    transports: Array.from(transportSet),
    efforts: Array.from(effortSet),
  }));
}

/**
 * Attribute token records to each saved step and provider/model pair. This is
 * intentionally separate from the run-wide provider/model aggregation: the
 * latter remains useful for compact summaries, while this preserves the step
 * boundary needed by the provenance table.
 */
export function aggregateStepModelUsage(report: PipelineReport): StepModelUsage[] {
  const groups = new Map<
    string,
    StepModelUsage & { transportSet: Set<string>; effortSet: Set<string> }
  >();

  for (const step of report.step_outputs) {
    const records: Array<StepCallRecord | undefined> = step.calls?.length
      ? step.calls
      : [undefined];
    for (const call of records) {
      const identity = usageIdentity(step, call);
      const key = [
        step.step_id.toLocaleLowerCase(),
        identity.provider.toLocaleLowerCase(),
        identity.model.toLocaleLowerCase(),
      ].join("\u0000");
      let group = groups.get(key);
      if (!group) {
        group = {
          step_id: step.step_id,
          step_label: step.step_label || step.step_id,
          provider: identity.provider,
          model: identity.model,
          transports: [],
          efforts: [],
          transportSet: new Set<string>(),
          effortSet: new Set<string>(),
          ...ZERO_USAGE,
        };
        groups.set(key, group);
      }
      addUsage(group, call ?? step);
      if (identity.transport) group.transportSet.add(identity.transport.toUpperCase());
      if (identity.effort) group.effortSet.add(identity.effort);
    }
  }

  return Array.from(groups.values()).map(({ transportSet, effortSet, ...group }) => ({
    ...group,
    transports: Array.from(transportSet),
    efforts: Array.from(effortSet),
  }));
}

function totalsForRows(rows: ProviderModelUsage[]): UsageTotals {
  const totals = { ...ZERO_USAGE };
  for (const row of rows) addUsage(totals, row);
  return totals;
}

function authoritativeTotals(
  summary?: RunSummary | null,
  manifest?: ProvenanceManifest | null,
): UsageTotals | null {
  if (summary) {
    const totals = {
      input_tokens: count(summary.input_tokens),
      output_tokens: count(summary.output_tokens),
      cached_input_tokens: count(summary.cached_input_tokens),
      cache_write_input_tokens: count(summary.cache_write_input_tokens),
    };
    if (hasUsage(totals)) return totals;
  }
  if (manifest?.usage) {
    const totals = { ...ZERO_USAGE };
    addUsage(totals, manifest.usage);
    if (hasUsage(totals)) return totals;
  }
  return null;
}

function sameUsage(left: UsageTotals, right: UsageTotals): boolean {
  return (
    left.input_tokens === right.input_tokens &&
    left.output_tokens === right.output_tokens &&
    left.cached_input_tokens === right.cached_input_tokens &&
    left.cache_write_input_tokens === right.cache_write_input_tokens
  );
}

function unique(values: string[]): string[] {
  return Array.from(new Set(values.filter(Boolean)));
}

export function buildRunProvenance({
  report,
  summary,
  manifest,
  runId,
  durationSecs,
}: {
  report: PipelineReport;
  summary?: RunSummary | null;
  manifest?: ProvenanceManifest | null;
  runId?: string | null;
  durationSecs?: number | null;
}): RunProvenance {
  const paper = isPaperOrientation(report.orientation) ? report.orientation : null;
  const usage = aggregateProviderModelUsage(report);
  const stepUsage = aggregateStepModelUsage(report);
  const attributedTotals = totalsForRows(usage);
  const totals = authoritativeTotals(summary, manifest) ?? attributedTotals;
  const fallbackProvider = summary?.provider || manifest?.provider || "Default";
  const providers = unique(
    usage.length ? usage.map((row) => row.provider) : [displayProvider(fallbackProvider)],
  );
  const models = unique(usage.map((row) => row.model));
  const resolvedDuration =
    summary?.duration_secs || manifest?.duration_secs || durationSecs || null;

  return {
    subject:
      paper?.metadata.title?.trim() ||
      summary?.title?.trim() ||
      manifest?.title?.trim() ||
      summary?.input_name?.trim() ||
      manifest?.input_path?.split(/[\\/]/).pop() ||
      "Pipeline report",
    authors: paper?.metadata.authors.filter(Boolean) ?? [],
    workflow:
      summary?.profile_name ||
      manifest?.profile_name ||
      summary?.profile_id ||
      manifest?.profile_id ||
      "Current workflow",
    completed: summary?.created || manifest?.created || report.report_date || "",
    duration_secs:
      resolvedDuration && Number.isFinite(resolvedDuration) && resolvedDuration > 0
        ? resolvedDuration
        : null,
    run_id: runId || summary?.run_id || manifest?.run_id || report.paper_hash || "",
    status:
      summary?.status ||
      manifest?.status ||
      (report.failed_steps?.length ? "partial" : "done"),
    usage,
    step_usage: stepUsage,
    totals,
    usage_matches_total: sameUsage(attributedTotals, totals),
    provider_summary: providers.join(", ") || "Default",
    model_summary: models.join(", ") || "Automatic",
  };
}

export function formatRunDuration(seconds?: number | null): string {
  if (!seconds || seconds <= 0 || !Number.isFinite(seconds)) return "—";
  const roundedSeconds = Math.round(seconds);
  const minutes = Math.floor(roundedSeconds / 60);
  const remainder = roundedSeconds % 60;
  return minutes ? `${minutes}m ${remainder}s` : `${remainder}s`;
}

export function formatRunDate(value?: string | null): string {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

export function formatTokens(value: number): string {
  return count(value).toLocaleString();
}

function escapeMarkdown(value: string): string {
  return value
    .replace(/\\/g, "\\\\")
    .replace(/([|*_`])/g, "\\$1")
    .replace(/\s*\n\s*/g, " ")
    .trim();
}

function modelUsageLabel(row: ProviderModelUsage): string {
  const transport = row.transports.length ? ` (${row.transports.join(" + ")})` : "";
  const effort = row.efforts.length ? row.efforts.join(" + ") : "not recorded";
  return `${row.provider}${transport} / ${row.model} / effort ${effort}`;
}

function tokenSummary(provenance: RunProvenance): string {
  const parts = [
    `${formatTokens(provenance.totals.input_tokens)} input`,
    `${formatTokens(provenance.totals.output_tokens)} output`,
    `${formatTokens(provenance.totals.cached_input_tokens)} cached`,
  ];
  if (provenance.totals.cache_write_input_tokens > 0) {
    parts.push(`${formatTokens(provenance.totals.cache_write_input_tokens)} cache write`);
  }
  return parts.join(" · ");
}

/** Build the compact provenance block prepended to the PDF export. */
export function provenanceMarkdown(provenance: RunProvenance): string {
  const modelSummary = provenance.usage.length
    ? provenance.usage.map(modelUsageLabel).join("; ")
    : `${provenance.provider_summary} / ${provenance.model_summary} / effort not recorded`;
  return [
    "# Pipeline",
    "",
    `**Document:** ${escapeMarkdown(provenance.subject)}`,
    "",
    `**Workflow:** ${escapeMarkdown(provenance.workflow)}`,
    "",
    `**Token usage:** ${escapeMarkdown(tokenSummary(provenance))}`,
    "",
    `**Models / providers:** ${escapeMarkdown(modelSummary)}`,
  ].join("\n");
}
