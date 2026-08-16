import type { PipelineConfig } from "./types";

export const AUTO_REVIEW_CONTRACT = "auto-review-v1";
export const ADAPTIVE_AGENT_COUNT_KEY = "x-pipeline-adaptive-agent-count";
export const MIN_ADAPTIVE_AGENTS = 2;
export const MAX_ADAPTIVE_AGENTS = 6;

export interface AdaptiveAgentRange {
  subjectMin: number;
  subjectMax: number;
  methodMin: number;
  methodMax: number;
  totalMin: number;
  totalMax: number;
}

const DEFAULT_RANGE: AdaptiveAgentRange = {
  subjectMin: 1,
  subjectMax: 2,
  methodMin: 1,
  methodMax: 4,
  totalMin: MIN_ADAPTIVE_AGENTS,
  totalMax: MAX_ADAPTIVE_AGENTS,
};

export function isAutoReview(config: PipelineConfig): boolean {
  return config.orientation_schema?.["x-pipeline-contract"] === AUTO_REVIEW_CONTRACT;
}

function boundedSchemaInteger(value: unknown, fallback: number, minimum: number, maximum: number): number {
  return typeof value === "number"
    && Number.isInteger(value)
    && value >= minimum
    && value <= maximum
    ? value
    : fallback;
}

export function adaptiveAgentRange(config: PipelineConfig): AdaptiveAgentRange {
  const reviewPlan = config.orientation_schema?.properties;
  const planSchema = reviewPlan && typeof reviewPlan === "object"
    ? (reviewPlan as Record<string, unknown>).review_plan
    : undefined;
  const planProperties = planSchema && typeof planSchema === "object"
    ? (planSchema as Record<string, unknown>).properties
    : undefined;
  const properties = planProperties && typeof planProperties === "object"
    ? planProperties as Record<string, unknown>
    : {};
  const subjectSchema = properties.subject_specialist_ids && typeof properties.subject_specialist_ids === "object"
    ? properties.subject_specialist_ids as Record<string, unknown>
    : {};
  const methodSchema = properties.method_specialist_ids && typeof properties.method_specialist_ids === "object"
    ? properties.method_specialist_ids as Record<string, unknown>
    : {};
  const subjectMin = boundedSchemaInteger(subjectSchema.minItems, DEFAULT_RANGE.subjectMin, 1, 2);
  const subjectMax = boundedSchemaInteger(subjectSchema.maxItems, DEFAULT_RANGE.subjectMax, subjectMin, 2);
  const methodMin = boundedSchemaInteger(methodSchema.minItems, DEFAULT_RANGE.methodMin, 1, 4);
  const methodMax = boundedSchemaInteger(methodSchema.maxItems, DEFAULT_RANGE.methodMax, methodMin, 4);
  return {
    subjectMin,
    subjectMax,
    methodMin,
    methodMax,
    totalMin: subjectMin + methodMin,
    totalMax: subjectMax + methodMax,
  };
}

/** Null means the router chooses any valid count in the profile's schema range. */
export function adaptiveAgentCount(config: PipelineConfig): number | null {
  const value = config.orientation_schema?.[ADAPTIVE_AGENT_COUNT_KEY];
  const range = adaptiveAgentRange(config);
  return typeof value === "number"
    && Number.isInteger(value)
    && value >= range.totalMin
    && value <= range.totalMax
    ? value
    : null;
}

export function adaptiveAgentCountLabel(
  count: number | null,
  range: AdaptiveAgentRange = DEFAULT_RANGE,
): string {
  return count === null ? `${range.totalMin}–${range.totalMax}` : String(count);
}

export function withAdaptiveAgentCount(
  config: PipelineConfig,
  count: number | null,
): PipelineConfig {
  const schema = { ...(config.orientation_schema ?? {}) };
  if (count === null) {
    delete schema[ADAPTIVE_AGENT_COUNT_KEY];
  } else {
    schema[ADAPTIVE_AGENT_COUNT_KEY] = count;
  }
  return { ...config, orientation_schema: schema };
}
