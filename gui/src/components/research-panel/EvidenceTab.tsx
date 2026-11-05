import { primaryButton } from "../../ui/classes";
import { researchClient } from "../../lib/workbench/researchClient";
import { importedLabel, plainLabel } from "./format";
import { operation } from "./useResearchPanelController";
import type { useResearchPanelController } from "./useResearchPanelController";
import type { ConversationSnapshot } from "../../lib/workbenchTypes";

type Panel = ReturnType<typeof useResearchPanelController>;
type Props = Pick<Panel, "busy" | "act" | "claim" | "setClaim"> & {
  workspaceId: string;
  snapshot: ConversationSnapshot | null;
  ledger: NonNullable<Panel["ledger"]>;
};

export default function EvidenceTab({
  busy,
  act,
  claim,
  setClaim,
  workspaceId,
  snapshot,
  ledger,
}: Props) {
  return (
    <div className="space-y-4">
      <div>
        <textarea
          aria-label="Claim text"
          value={claim}
          onChange={(event) => setClaim(event.target.value)}
          rows={3}
          placeholder="Record a claim for review…"
          className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
        />
        <button
          type="button"
          disabled={!claim.trim() || busy}
          onClick={() =>
            void act(async () => {
              await researchClient.proposeClaim({
                workspaceId,
                paperId: snapshot?.session.paperId ?? null,
                claim,
                kind: "research_claim",
                origin: "user",
                operationId: operation("claim"),
              });
              setClaim("");
            })
          }
          className={`mt-2 ${primaryButton}`}
        >
          Add proposed claim
        </button>
      </div>
      {(ledger.unsupportedAcceptedClaims.length > 0 ||
        ledger.staleClaims.length > 0) && (
        <p className="rounded border border-amber-200 bg-amber-50 p-2 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-200">
          {ledger.unsupportedAcceptedClaims.length} accepted claim(s) lack
          current supporting evidence that you confirmed or a check passed;{" "}
          {ledger.staleClaims.length} claim(s) have stale evidence.
        </p>
      )}
      <div className="space-y-2">
        {ledger.claims.map((item) => (
          <div
            key={item.id}
            className="rounded border bg-white p-3 dark:bg-neutral-950"
          >
            <div className="text-[11px] text-gray-500">
              {plainLabel(item.workflowState)} ·{" "}
              {item.importedStatus ? "Imported" : plainLabel(item.origin)} · v
              {item.version}
            </div>
            {item.importedStatus && (
              <p className="text-[11px] text-amber-700 dark:text-amber-300">
                {importedLabel(item.importedStatus)}
              </p>
            )}
            <p className="mt-1">{item.claim}</p>
            {item.workflowState !== "accepted" && (
              <button
                type="button"
                onClick={() =>
                  void act(async () => {
                    await researchClient.setClaimState({
                      claimId: item.id,
                      state: "accepted",
                      operationId: operation("accept-claim"),
                    });
                  })
                }
                className="mt-2 rounded border px-2 py-1 text-[11px]"
              >
                Accept claim
              </button>
            )}
          </div>
        ))}
      </div>
      <section>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
          Evidence links
        </h3>
        <div className="mt-2 space-y-2">
          {ledger.evidence.map((item) => (
            <div
              key={item.id}
              className="rounded border bg-white p-2 text-xs dark:bg-neutral-950"
            >
              <div>
                {plainLabel(item.relation)} · {plainLabel(item.targetType)}
              </div>
              <div
                className={
                  item.freshness === "stale" ? "text-red-600" : "text-gray-500"
                }
              >
                {plainLabel(item.assessment)}
                {item.importedStatus || item.assessor === "user"
                  ? ""
                  : ` by ${item.assessor}`}{" "}
                · {plainLabel(item.freshness)}
                {item.staleReason ? ` — ${item.staleReason}` : ""}
              </div>
              {item.importedStatus && (
                <p className="text-amber-700 dark:text-amber-300">
                  {importedLabel(item.importedStatus)}
                </p>
              )}
              {item.assessment !== "human_confirmed" && (
                <button
                  type="button"
                  onClick={() =>
                    void act(async () => {
                      await researchClient.confirmEvidence({
                        evidenceId: item.id,
                        operationId: operation("confirm-evidence"),
                      });
                    })
                  }
                  className="mt-1 rounded border px-2 py-1 text-[11px]"
                >
                  Confirm evidence
                </button>
              )}
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
