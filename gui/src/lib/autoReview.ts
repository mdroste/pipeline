import type { PipelineConfig } from "./types";

export const AUTO_REVIEW_CONTRACT = "auto-review-v2";
export const ADAPTIVE_AGENT_COUNT_KEY = "x-pipeline-adaptive-agent-count";
export const MIN_ADAPTIVE_AGENTS = 2;
export const MAX_ADAPTIVE_AGENTS = 6;

export function isAutoReview(config: PipelineConfig): boolean {
  return config.orientation_schema?.["x-pipeline-contract"] === AUTO_REVIEW_CONTRACT;
}

/** Null means the router chooses any valid count in the 2–6 range. */
export function adaptiveAgentCount(config: PipelineConfig): number | null {
  const value = config.orientation_schema?.[ADAPTIVE_AGENT_COUNT_KEY];
  return typeof value === "number"
    && Number.isInteger(value)
    && value >= MIN_ADAPTIVE_AGENTS
    && value <= MAX_ADAPTIVE_AGENTS
    ? value
    : null;
}

export function adaptiveAgentCountLabel(count: number | null): string {
  return count === null ? `${MIN_ADAPTIVE_AGENTS}–${MAX_ADAPTIVE_AGENTS}` : String(count);
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
