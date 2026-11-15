import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DiscoveryDefinition } from "../../lib/discoveryClient";
import { ReviewEstimate } from "../tasks/Budget";
interface Preview {
  screenedAtLimit: number;
  firstAssessment: number | null;
  shortlistMin: number;
  portfolioMin: number;
  conditionalHigh: number;
  reachable: boolean;
}
export default function BudgetPreview({
  definition: d,
}: {
  definition: DiscoveryDefinition;
}) {
  const [preview, setPreview] = useState<Preview | null>(null),
    [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    setPreview(null);
    setError("");
    const timer = setTimeout(() => {
      void invoke<Preview>("discovery_budget_preview", { definition: d })
        .then((r) => {
          if (live) setPreview(r);
        })
        .catch((e) => {
          if (live) setError(String(e));
        });
    }, 100);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [d]);
  return (
    <section aria-label="Budget preview">
      <h4>Budget preview</h4>
      {preview ? (
        <>
          <p>
            The shortlist and {d.paperCount} ranked paper
            {d.paperCount !== 1 ? "s" : ""} take an estimated{" "}
            <strong>
              {preview.portfolioMin}–{preview.conditionalHigh} actions
            </strong>
            . Your limit is {d.maxActions}.
          </p>
          <p>
            The shortlist alone needs at least {preview.shortlistMin} actions.
            Allow extra actions for fixes and retries.
          </p>
          {!preview.reachable && (
            <p>
              Raise the limit to at least {preview.portfolioMin} for the full
              run, or keep this limit for partial results.
            </p>
          )}
          {preview.screenedAtLimit < d.candidateCount && (
            <p>
              At this limit, Pipeline can screen at most{" "}
              {preview.screenedAtLimit} of {d.candidateCount} projects.
            </p>
          )}
          {preview.firstAssessment !== null &&
            d.maxActions < preview.firstAssessment && (
              <p>
                The earliest project assessment is action{" "}
                {preview.firstAssessment}.
              </p>
            )}
          {d.reviewProfileId && <p>Each Review counts as one action.</p>}
        </>
      ) : (
        <p>
          {error
            ? "Budget preview unavailable: " + error
            : "Calculating action range…"}
        </p>
      )}
      <ReviewEstimate profile={d.reviewProfileId} />
    </section>
  );
}
