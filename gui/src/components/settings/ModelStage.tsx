import type { ReactNode } from "react";
import type { ModelCatalog, ModelSelection, Settings } from "../../lib/types";
import {
  PROVIDER_LABELS,
  providerTransport,
  type Provider,
} from "../../lib/providers";

export default function ModelStage({
  title,
  agents,
  models = {},
  efforts = {},
  settings,
  catalogs,
  children,
}: {
  title: string;
  agents: string[];
  models?: Record<string, ModelSelection>;
  efforts?: Record<string, string>;
  settings: Settings;
  catalogs: Record<string, ModelCatalog>;
  children: ReactNode;
}) {
  return (
    <details className="settings-model-stage">
      <summary>
        <span className="settings-row-label">{title}</span>
        <span className="settings-model-summary">
          {agents.map((agent) => {
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
                    ? `Role: ${selected.role}`
                    : "Provider default"}{" "}
                · {efforts[key] || efforts[agent] || "Default reasoning"}
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
