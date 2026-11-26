import { useEffect, useState, type ReactNode } from "react";
import type { DepsReport, Settings } from "../lib/settingsTypes";
import type { RunModelOverrides } from "../lib/reviewModelTypes";
import { withoutDefaultModel } from "../lib/modelChoice";
import { providerAvailability } from "../lib/connections";
import { providerTransport } from "../lib/providers";
import {
  changeReviewTeam,
  defaultReviewModel,
  hasSeparateParallelChoice,
  needsReviewTeam,
  primaryReview,
  withoutCombiningOverride,
  withoutProvider,
} from "../lib/reviewTeam";
import { Icon } from "../ui/icons";
import ReviewModelRows from "./review-setup/ReviewModelRows";
import ReviewInspectorTabs from "./review-setup/ReviewInspectorTabs";
import ReviewRoster from "./review-setup/ReviewRoster";
import { useReviewModelSetup } from "./review-setup/useReviewModelSetup";

interface Props {
  value: RunModelOverrides | null;
  disabled?: boolean;
  dependencies?: DepsReport | null;
  localLlmActive?: boolean;
  refreshKey?: number;
  workflowSteps?: ReactNode;
  onConnectModel?: () => void;
  onChange: (value: RunModelOverrides | null) => void;
}

/** Primary handles the whole review; extra models provide additional opinions. */
export default function RunModelChoice({
  value,
  disabled = false,
  dependencies,
  localLlmActive = false,
  refreshKey = 0,
  workflowSteps,
  onConnectModel,
  onChange,
}: Props) {
  const [tab, setTab] = useState("models");
  const {
    settings,
    config,
    catalogs,
    providers,
    catalogErrors,
    roster,
    loadError,
    rosterError,
    rosterLoading,
    retry,
  } = useReviewModelSetup(value, refreshKey, localLlmActive);
  useEffect(() => {
    if (!settings || !config) return;
    const next = withoutCombiningOverride(value);
    if (needsReviewTeam(next)) onChange(primaryReview(settings, next));
    else if (next !== value) onChange(next);
  }, [settings, config, value, onChange]);

  if (!settings || !config)
    return (
      <section
        aria-label="Model settings"
        className="review-setup-card review-model-card"
      >
        <h2>Model settings</h2>
        {loadError ? (
          <div role="alert">
            <p>Couldn't load model choices. {loadError}</p>
            <button onClick={retry}>Retry model choices</button>
          </div>
        ) : (
          <p role="status">Loading model choices…</p>
        )}
      </section>
    );

  const selection = needsReviewTeam(value)
    ? primaryReview(settings, value)
    : withoutCombiningOverride(value)!;
  const defaultModel = defaultReviewModel(settings);
  const primary = selection.review ?? defaultModel;
  const primaryKey = `${primary.provider}:${providerTransport(settings, primary.provider)}`;
  const primaryModels = primary.model
    ? { [primaryKey]: { mode: "pinned" as const, model: primary.model } }
    : {};
  const primaryEfforts = primary.effort ? { [primaryKey]: primary.effort } : {};
  const runSettings: Settings = {
    ...settings,
    preferred_provider: primary.provider,
    default_model: primary.provider === "local" ? "" : primary.model,
    default_effort: primary.effort,
    local_model:
      primary.provider === "local" && primary.model
        ? primary.model
        : settings.local_model,
  };
  const noModelsConnected = providers.every(
    (provider) =>
      providerAvailability(provider, settings, dependencies) === "missing",
  );
  const separateParallel = hasSeparateParallelChoice(selection, settings);
  // Pipeline checks make no model call, and an editorial (Declaude) stage keeps
  // one editor each, so neither can take added models.
  const canAddReviewers = config.steps.some(
    (step) =>
      step.enabled &&
      step.phase === "parallel" &&
      step.kind !== "analyzer" &&
      typeof step.output_schema?.["x-pipeline-editorial-stage"] !== "string",
  );
  const team = separateParallel
    ? [primary.provider]
    : selection.agents.length
      ? selection.agents
      : [primary.provider];
  const hasExceptions =
    !selection.clear_settings_exceptions ||
    !selection.override_workflow_pins ||
    separateParallel;
  const changed =
    primary.provider !== defaultModel.provider ||
    primary.model !== defaultModel.model ||
    primary.effort !== defaultModel.effort ||
    !!selection.agents.length ||
    hasExceptions;
  const change = (patch: Partial<RunModelOverrides>) =>
    onChange({ ...selection, review: primary, ...patch });
  const common = {
    settings: withoutDefaultModel(settings),
    catalogs,
    providers,
    dependencies,
    disabled,
  };
  const tabs = [
    { id: "models", label: "Models" },
    ...(!noModelsConnected ? [{ id: "advanced", label: "Advanced" }] : []),
    ...(workflowSteps ? [{ id: "steps", label: "Steps" }] : []),
  ];
  const activeTab = tabs.some((item) => item.id === tab) ? tab : "models";

  return (
    <section
      aria-label="Model settings"
      className="review-setup-card review-model-card"
    >
      <div className="review-section-heading">
        <h2>Model settings</h2>
        {changed && !noModelsConnected && (
          <button
            type="button"
            disabled={disabled}
            onClick={() => onChange(primaryReview(settings, selection))}
            className="ui-button ui-button--quiet ui-button--sm"
          >
            Reset
          </button>
        )}
      </div>
      <ReviewInspectorTabs tabs={tabs} value={activeTab} onChange={setTab}>
        {!noModelsConnected && catalogErrors.length > 0 && (
          <div role="alert" className="review-inline-error">
            <p>
              Couldn't load model lists for {catalogErrors.join(", ")}. Saved
              choices are retained.
            </p>
            <button onClick={retry}>Retry model lists</button>
          </div>
        )}
        {!noModelsConnected && rosterError && (
          <div role="alert" className="review-inline-error">
            <p>Couldn't check models for each step. {rosterError}</p>
            <button onClick={retry}>Retry model check</button>
          </div>
        )}
        {activeTab === "models" && noModelsConnected && (
          <div role="alert" className="review-model-empty">
            <p>No models connected</p>
            {onConnectModel ? (
              <button
                type="button"
                className="ui-button"
                disabled={disabled}
                onClick={onConnectModel}
              >
                Connect a model
                <Icon name="arrow" className="h-3.5 w-3.5" />
              </button>
            ) : (
              <span>Connect a model in Settings to start a review.</span>
            )}
          </div>
        )}
        {activeTab === "models" && !noModelsConnected && (
          <>
            {(hasExceptions || team.length > 1) && (
              <p className="review-team-help">
                {hasExceptions
                  ? "Your primary model, with advanced assignments."
                  : "Primary leads the review and combines feedback."}
              </p>
            )}
            <ReviewModelRows
              {...common}
              label="Review models"
              team
              multi={canAddReviewers && !separateParallel}
              agents={team}
              modelOverrides={{
                ...withoutProvider(
                  separateParallel ? {} : selection.model_overrides,
                  primary.provider,
                ),
                ...primaryModels,
              }}
              effortOverrides={{
                ...withoutProvider(
                  separateParallel ? {} : selection.effort_overrides,
                  primary.provider,
                ),
                ...primaryEfforts,
              }}
              onChange={(next) => {
                const updated = changeReviewTeam(selection, next, settings);
                onChange(
                  separateParallel
                    ? { ...selection, review: updated.review }
                    : updated,
                );
              }}
            />
            {team.length > 1 && (
              <p className="review-meta">
                Each additional model runs the parallel review steps again. The
                primary model combines their feedback. This adds model work; it
                does not set the number of specialist roles.
              </p>
            )}
            {hasExceptions && (
              <button
                type="button"
                className="review-inspect-exceptions"
                onClick={() => setTab("advanced")}
              >
                Custom model assignments · Advanced
              </button>
            )}
          </>
        )}
        {activeTab === "advanced" && (
          <>
            <div className="review-inheritance">
              <label>
                <input
                  type="checkbox"
                  checked={!selection.clear_settings_exceptions}
                  disabled={disabled}
                  onChange={(event) =>
                    change({ clear_settings_exceptions: !event.target.checked })
                  }
                />
                Use saved Settings model choices
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={!selection.override_workflow_pins}
                  disabled={disabled}
                  onChange={(event) =>
                    change({ override_workflow_pins: !event.target.checked })
                  }
                />
                Use design-pinned models
              </label>
            </div>
            {separateParallel && (
              <div className="review-custom-parallel">
                <div className="review-section-heading">
                  <h3>Custom parallel assignments</h3>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() =>
                      change({
                        agents: [],
                        model_overrides: {},
                        effort_overrides: {},
                      })
                    }
                  >
                    Use primary model
                  </button>
                </div>
                <ReviewModelRows
                  {...common}
                  settings={runSettings}
                  label="Custom review models"
                  multi
                  agents={selection.agents}
                  modelOverrides={selection.model_overrides}
                  effortOverrides={selection.effort_overrides}
                  onChange={(next) =>
                    change({
                      agents: next.agents,
                      model_overrides: next.modelOverrides,
                      effort_overrides: next.effortOverrides,
                    })
                  }
                />
              </div>
            )}
            {rosterLoading ? (
              <p role="status" className="review-meta">
                Checking models for each step…
              </p>
            ) : (
              roster && (
                <ReviewRoster
                  roster={roster}
                  settings={runSettings}
                  catalogs={catalogs}
                  pageSize={separateParallel ? 1 : 2}
                />
              )
            )}
          </>
        )}
        {activeTab === "steps" && workflowSteps}
      </ReviewInspectorTabs>
    </section>
  );
}
