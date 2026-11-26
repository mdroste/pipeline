import type { Settings } from "./settingsTypes";
import type {
  ModelSelection,
  ReviewModelChoice,
  RunModelOverrides,
} from "./reviewModelTypes";
import { providerTransport } from "./providers";

export function defaultReviewModel(settings: Settings): ReviewModelChoice {
  const provider = settings.preferred_provider || "claude";
  return {
    provider,
    model:
      (provider === "local" ? settings.local_model : settings.default_model) ??
      "",
    effort: settings.default_effort ?? "",
  };
}

/** An extraction choice alone does not opt into a custom model arrangement. */
export function needsReviewTeam(value: RunModelOverrides | null): boolean {
  return (
    !value?.review &&
    !value?.agents.length &&
    value?.clear_settings_exceptions === undefined &&
    value?.override_workflow_pins === undefined
  );
}

/** New Review inherits combining from Primary; discard older draft overrides. */
export function withoutCombiningOverride(
  value: RunModelOverrides | null,
): RunModelOverrides | null {
  if (
    !value ||
    !(
      "merge_agent" in value ||
      "merge_model_overrides" in value ||
      "merge_effort_overrides" in value
    )
  )
    return value;
  const next = { ...value };
  delete next.merge_agent;
  delete next.merge_model_overrides;
  delete next.merge_effort_overrides;
  return next;
}

export function primaryReview(
  settings: Settings,
  value: RunModelOverrides | null,
): RunModelOverrides {
  return {
    review: defaultReviewModel(settings),
    agents: [],
    model_overrides: {},
    effort_overrides: {},
    clear_settings_exceptions: true,
    override_workflow_pins: true,
    ...(value?.extraction_method
      ? { extraction_method: value.extraction_method }
      : {}),
  };
}

export function withoutProvider<T>(
  map: Record<string, T>,
  provider: string,
): Record<string, T> {
  return Object.fromEntries(
    Object.entries(map).filter(
      ([key]) => key !== provider && !key.startsWith(`${provider}:`),
    ),
  );
}

export interface ReviewTeamRows {
  agents: string[];
  modelOverrides: Record<string, ModelSelection>;
  effortOverrides: Record<string, string>;
}

/** Keep older independent parallel choices in Advanced until explicitly reset. */
export function hasSeparateParallelChoice(
  value: RunModelOverrides,
  settings: Settings,
): boolean {
  if (!value.agents.length) return false;
  const primary = value.review ?? defaultReviewModel(settings);
  const key = `${primary.provider}:${providerTransport(settings, primary.provider)}`;
  const model =
    value.model_overrides[key] ?? value.model_overrides[primary.provider];
  const effort =
    value.effort_overrides[key] ?? value.effort_overrides[primary.provider];
  return (
    value.agents[0] !== primary.provider ||
    (!!model && (model.mode !== "pinned" || model.model !== primary.model)) ||
    (effort !== undefined && effort !== primary.effort)
  );
}

/** The first row always supplies the shared default, including combining.
 * Only extra reviewers belong in the per-provider maps, so changing the
 * primary never leaves a stale parallel or combining copy behind. */
export function changeReviewTeam(
  value: RunModelOverrides,
  rows: ReviewTeamRows,
  settings: Settings,
): RunModelOverrides {
  const provider = rows.agents[0];
  const key = `${provider}:${providerTransport(settings, provider)}`;
  const model = rows.modelOverrides[key] ?? rows.modelOverrides[provider];
  return {
    ...withoutCombiningOverride(value),
    review: {
      provider,
      model: model?.mode === "pinned" ? model.model : "",
      effort: rows.effortOverrides[key] ?? rows.effortOverrides[provider] ?? "",
    },
    agents: rows.agents.length > 1 ? rows.agents : [],
    model_overrides: withoutProvider(rows.modelOverrides, provider),
    effort_overrides: withoutProvider(rows.effortOverrides, provider),
  };
}
