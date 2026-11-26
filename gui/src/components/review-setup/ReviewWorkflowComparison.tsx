import { useState } from "react";
import type { ProfileSummary } from "../../lib/workflowTypes";
import { workflowDescription } from "../../lib/workflowDescriptions";
import ReviewSetupDialog from "./ReviewSetupDialog";

export default function ReviewWorkflowComparison({
  profiles,
  activeId,
  disabled,
  onSelect,
  onClose,
}: {
  profiles: ProfileSummary[];
  activeId: string;
  disabled: boolean;
  onSelect: (id: string) => void;
  onClose: () => void;
}) {
  const [page, setPage] = useState(0);
  const pages = Math.max(1, Math.ceil(profiles.length / 3));
  const current = Math.min(page, pages - 1);
  return (
    <ReviewSetupDialog title="Compare review designs" onClose={onClose}>
      <div className="review-workflow-choices">
        {profiles.slice(current * 3, current * 3 + 3).map((profile) => {
          const description = workflowDescription(profile);
          return (
            <button
              key={profile.id}
              type="button"
              disabled={disabled || !!profile.invalid}
              aria-pressed={profile.id === activeId}
              onClick={() => onSelect(profile.id)}
            >
              <strong>
                {profile.name}
                {profile.id === activeId ? " · Selected" : ""}
              </strong>
              <span>{description.outcome}</span>
              <small>
                {description.category} · {description.inputs} ·{" "}
                {profile.step_count} configured steps
                {profile.invalid ? ` · ${profile.invalid}` : ""}
              </small>
            </button>
          );
        })}
      </div>
      {pages > 1 && (
        <div className="review-pagination">
          <button
            type="button"
            disabled={!current}
            onClick={() => setPage(current - 1)}
          >
            Previous
          </button>
          <span aria-live="polite">
            {current + 1} / {pages}
          </span>
          <button
            type="button"
            disabled={current === pages - 1}
            onClick={() => setPage(current + 1)}
          >
            Next
          </button>
        </div>
      )}
    </ReviewSetupDialog>
  );
}
