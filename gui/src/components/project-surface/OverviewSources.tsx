import type {
  ProjectSignals,
  SignalSource,
} from "../../lib/project-overview/types";
import { projectDate } from "../../lib/projectIndex";
const labels: Record<SignalSource, string> = {
  builds: "Builds",
  reviews: "Reviews",
  responses: "Responses",
  automations: "Automations",
  repository: "Git",
};
export default function OverviewSources({
  signals,
  onRefresh,
  tasksPartial,
}: {
  signals: ProjectSignals;
  onRefresh: (source: SignalSource) => void;
  tasksPartial: boolean;
}) {
  return (
    <details className="mt-3 text-xs">
      <summary>Sources and freshness</summary>
      {tasksPartial && (
        <p className="my-2">
          Attention shows the first page of action items. Open All action items
          to load more; this is not the project total.
        </p>
      )}
      {(Object.keys(labels) as SignalSource[]).map((key) => {
        const source = signals.sources?.[key];
        return (
          <div className="my-3" key={key}>
            <div className="overview-actions">
              <strong>{labels[key]}</strong>
              <span>
                {!source
                  ? "Not checked"
                  : source.state === "loading"
                    ? "Checking…"
                    : source.state === "not-applicable"
                      ? "Not applicable"
                      : source.state === "error"
                        ? "Unavailable"
                        : source.complete
                          ? "Loaded"
                          : "Partial"}
              </span>
              <button
                type="button"
                className="project-home-link"
                disabled={
                  source?.state === "loading" ||
                  source?.state === "not-applicable"
                }
                onClick={() => onRefresh(key)}
              >
                Refresh {labels[key]}
              </button>
            </div>
            {source && (
              <p>
                {source.scope}
                {source.fetchedAt
                  ? ` · Last loaded ${projectDate(source.fetchedAt)}`
                  : ""}
                {source.state === "error" && source.fetchedAt
                  ? " · Showing last known results"
                  : ""}
              </p>
            )}
            {source?.error && <p role="status">{source.error}</p>}
          </div>
        );
      })}
    </details>
  );
}
