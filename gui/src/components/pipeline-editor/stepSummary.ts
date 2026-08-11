import type {
  PipelineConfig,
  PrimaryArtifactPart,
  StepConfig,
} from "../../lib/types";

function naturalList(items: string[]): string {
  if (items.length === 0) return "";
  if (items.length === 1) return items[0];
  if (items.length === 2) return `${items[0]} and ${items[1]}`;
  return `${items.slice(0, -1).join(", ")}, and ${items[items.length - 1]}`;
}

function providerLabel(provider: string): string {
  return provider.charAt(0).toUpperCase() + provider.slice(1);
}

function isIssuesSchema(schema: Record<string, unknown> | null | undefined): boolean {
  if (!schema || schema.type !== "object") return false;
  const properties = schema.properties;
  return !!properties && typeof properties === "object" && "issues" in properties;
}

function inputDescriptions(step: StepConfig, config: PipelineConfig): string[] {
  const labels = new Map(config.steps.map((candidate) => [candidate.id, candidate.label]));
  const descriptions: string[] = [];

  for (const selector of step.context.include) {
    if (selector.kind === "primary") {
      const allDocumentParts: PrimaryArtifactPart[] = ["text", "structure", "visuals", "source"];
      const hasFullDocument = allDocumentParts.every((part) => selector.parts.includes(part));
      if (hasFullDocument) {
        descriptions.push("the full primary input");
        continue;
      }
      if (selector.parts.includes("text")) descriptions.push("readable input text");
      if (selector.parts.includes("structure")) descriptions.push("document structure");
      if (selector.parts.includes("visuals")) descriptions.push("pages and figures");
      if (selector.parts.includes("source")) descriptions.push("the original source");
    }
    if (selector.kind === "survey") descriptions.push("the orientation map");
    if (selector.kind === "named_input") descriptions.push(`the “${selector.key}” input`);
    if (selector.kind === "step") {
      const producer = labels.get(selector.step) ?? selector.step;
      if (selector.parts.includes("report")) descriptions.push(`the ${producer} report`);
      if (selector.parts.includes("files")) descriptions.push(`files from ${producer}`);
    }
  }

  return descriptions;
}

export interface StepSummary {
  sentence: string;
  compact: string;
  maxCalls: number;
}

/** Describe the effective step configuration without exposing config jargon. */
export function describeStep(step: StepConfig, config: PipelineConfig): StepSummary {
  const providers = step.agents.length
    ? naturalList(step.agents.map(providerLabel))
    : "the profile’s default provider";
  const timing = step.phase === "parallel"
    ? "Runs independently"
    : "Runs after its selected dependencies";
  const condition = step.run_if?.kind === "output_matches"
    ? " when an upstream report matches its condition"
    : step.run_if?.kind === "survey_path"
      ? " when the orientation map matches its condition"
      : "";
  const inputs = inputDescriptions(step, config);
  const receives = inputs.length
    ? `Receives ${naturalList(inputs)}.`
    : "Receives no workflow artifacts.";
  const output = isIssuesSchema(step.output_schema)
    ? "Produces structured issues JSON."
    : step.output_schema
      ? "Produces schema-validated JSON."
      : "Produces a report.";
  const maxUnits = step.for_each?.max ?? 1;
  const agentCount = Math.max(1, step.agents.length);
  const maxCalls = maxUnits * agentCount;
  const fanOut = step.for_each
    ? ` Fans out over up to ${step.for_each.max} files matching ${step.for_each.glob}.`
    : "";
  const calls = maxCalls === 1
    ? "One provider call before retries."
    : `Up to ${maxCalls} provider calls before retries.`;
  const disabled = step.enabled ? "" : "Disabled. ";

  return {
    sentence: `${disabled}${timing}${condition} using ${providers}. ${receives} ${output}${fanOut} ${calls}`,
    compact: `${step.enabled ? timing : "Disabled"} · ${providers} · ${maxCalls} ${maxCalls === 1 ? "call" : "calls"}`,
    maxCalls,
  };
}
