import { useState } from "react";
import { rosterModelSummary, rosterSourceLabel } from "../../lib/modelChoice";
import type { ModelCatalog, RosterEntry } from "../../lib/reviewModelTypes";
import type { Settings } from "../../lib/settingsTypes";

export default function ReviewRoster({
  roster,
  settings,
  catalogs,
  pageSize = 3,
}: {
  roster: RosterEntry[];
  settings: Settings;
  catalogs: Record<string, ModelCatalog>;
  pageSize?: number;
}) {
  const [page, setPage] = useState(0);
  const pages = Math.max(1, Math.ceil(roster.length / pageSize));
  const current = Math.min(page, pages - 1);
  return (
    <div className="review-roster">
      <div className="review-section-heading">
        <h3>Every model this review calls</h3>
        {pages > 1 && (
          <div className="review-pagination">
            <button
              type="button"
              aria-label="Previous models"
              disabled={!current}
              onClick={() => setPage(current - 1)}
            >
              ‹
            </button>
            <span aria-live="polite">
              {current + 1} / {pages}
            </span>
            <button
              type="button"
              aria-label="Next models"
              disabled={current === pages - 1}
              onClick={() => setPage(current + 1)}
            >
              ›
            </button>
          </div>
        )}
      </div>
      <ul aria-label="Every model this review calls">
        {roster
          .slice(current * pageSize, (current + 1) * pageSize)
          .map((entry) => (
            <li
              key={`${entry.stage}-${entry.step_ids.join(",")}-${entry.provider}`}
            >
              <div>
                <strong title={entry.step_labels.join(", ") || entry.label}>
                  {entry.step_labels.length > 2
                    ? `${entry.step_labels.length} ${entry.label.toLowerCase()}`
                    : entry.step_labels.join(", ") || entry.label}
                </strong>
                <span>{rosterSourceLabel(entry)}</span>
              </div>
              <p title={rosterModelSummary(settings, entry, catalogs)}>
                {rosterModelSummary(settings, entry, catalogs)}
              </p>
              {entry.note && (
                <p className="review-roster-note" title={entry.note}>
                  {entry.note}
                </p>
              )}
            </li>
          ))}
      </ul>
    </div>
  );
}
