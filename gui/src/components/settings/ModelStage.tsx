import type { ReactNode } from "react";
import type { ModelCatalog, ModelSelection, Settings } from "../../lib/types";
import {
  inheritedModelLabel,
  PROVIDER_LABELS,
  providerTransport,
  type Provider,
} from "../../lib/providers";

const effortLabel = (effort: string | undefined) =>
  effort
    ? effort.charAt(0).toUpperCase() + effort.slice(1)
    : "Default thinking";

export default function ModelStage({
  title,
  agents,
  models = {},
  efforts = {},
  settings,
  catalogs,
  inherited,
  children,
}: {
  title: string;
  agents: string[];
  models?: Record<string, ModelSelection>;
  efforts?: Record<string, string>;
  settings: Settings;
  catalogs: Record<string, ModelCatalog>;
  /** Shown instead of a provider list while the stage follows something else. */
  inherited?: string;
  children: ReactNode;
}) {
  return (
    <details className="settings-model-stage">
      <summary>
        <span className="settings-row-label">{title}</span>
        <span className="settings-model-summary">
          {inherited ??
            agents.map((agent) => {
              const key = `${agent}:${providerTransport(settings, agent)}`;
              const selected = models[key] ?? models[agent];
              const catalog = catalogs[agent];
              const exact = selected?.mode === "pinned" ? selected.model : null;
              const model = catalog?.models.find((model) => model.id === exact);
              return (
                <span key={agent}>
                  {PROVIDER_LABELS[agent as Provider] ?? agent} ·{" "}
                  {exact
                    ? `${model?.display_name || exact}${catalog && !model ? " (unavailable)" : ""}`
                    : selected?.mode === "role"
                      ? (catalog?.roles.find(
                          (role) => role.id === selected.role,
                        )?.label ?? selected.role)
                      : inheritedModelLabel(settings, agent, catalog)}{" "}
                  ·{" "}
                  {effortLabel(
                    efforts[key] ||
                      efforts[agent] ||
                      (agent === settings.preferred_provider
                        ? settings.default_effort
                        : ""),
                  )}
                </span>
              );
            })}
        </span>
        <span className="settings-stage-edit">Edit</span>
      </summary>
      <div className="settings-stage-body">{children}</div>
    </details>
  );
}
