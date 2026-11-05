import type { ReactNode } from "react";
import type { ModelCatalog, ModelSelection, Settings } from "../../lib/types";
import { stageModelSummary } from "../../lib/modelChoice";

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
            agents.map((agent) => (
              <span key={agent}>
                {stageModelSummary(settings, agent, models, efforts, catalogs)}
              </span>
            ))}
        </span>
        <span className="settings-stage-edit">Edit</span>
      </summary>
      <div className="settings-stage-body">{children}</div>
    </details>
  );
}
