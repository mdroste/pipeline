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
): { provider: string; model: string; transport: string } {
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
    ProviderModelUsage & { transportSet: Set<string> }
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
          transportSet: new Set<string>(),
          ...ZERO_USAGE,
        };
        groups.set(key, group);
      }
      addUsage(group, call ?? step);
      if (identity.transport) group.transportSet.add(identity.transport.toUpperCase());
    }
  }

  return Array.from(groups.values()).map(({ transportSet, ...group }) => ({
    ...group,
    transports: Array.from(transportSet),
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

/** Build the self-contained masthead prepended to the PDF export. */
export function provenanceMarkdown(provenance: RunProvenance): string {
  const lines = [
    "# Referee report",
    "",
    `Review of **${escapeMarkdown(provenance.subject)}**`,
  ];
  if (provenance.authors.length) {
    lines.push("", `Authors: ${escapeMarkdown(provenance.authors.join(", "))}`);
  }
  lines.push(
    "",
    "## Run provenance",
    "",
    "| Workflow | Completed | Duration | Run |",
    "| --- | --- | ---: | --- |",
    `| ${escapeMarkdown(provenance.workflow)} | ${escapeMarkdown(formatRunDate(provenance.completed))} | ${formatRunDuration(provenance.duration_secs)} | ${escapeMarkdown(provenance.run_id || "Unsaved")} |`,
    "",
  );

  const showCacheWrite = provenance.totals.cache_write_input_tokens > 0;
  const usageHeader = showCacheWrite
    ? "| Provider | Model | Input | Output | Cached input | Cache write |"
    : "| Provider | Model | Input | Output | Cached input |";
  const usageRule = showCacheWrite
    ? "| --- | --- | ---: | ---: | ---: | ---: |"
    : "| --- | --- | ---: | ---: | ---: |";
  lines.push(usageHeader, usageRule);
  for (const row of provenance.usage) {
    const transport = row.transports.length ? ` (${row.transports.join(" + ")})` : "";
    const cells = [
      escapeMarkdown(`${row.provider}${transport}`),
      escapeMarkdown(row.model),
      formatTokens(row.input_tokens),
      formatTokens(row.output_tokens),
      formatTokens(row.cached_input_tokens),
    ];
    if (showCacheWrite) cells.push(formatTokens(row.cache_write_input_tokens));
    lines.push(`| ${cells.join(" | ")} |`);
  }
  if (!provenance.usage.length) {
    const cells = ["Not reported", "Not reported", "0", "0", "0"];
    if (showCacheWrite) cells.push("0");
    lines.push(`| ${cells.join(" | ")} |`);
  }
  const totalCells = [
    "**Run total**",
    "",
    `**${formatTokens(provenance.totals.input_tokens)}**`,
    `**${formatTokens(provenance.totals.output_tokens)}**`,
    `**${formatTokens(provenance.totals.cached_input_tokens)}**`,
  ];
  if (showCacheWrite) {
    totalCells.push(`**${formatTokens(provenance.totals.cache_write_input_tokens)}**`);
  }
  lines.push(`| ${totalCells.join(" | ")} |`);
  if (!provenance.usage_matches_total) {
    lines.push(
      "",
      "_The run total includes setup calls not attached to a saved report step, or reused step provenance from an earlier run._",
    );
  }
  return lines.join("\n");
}
