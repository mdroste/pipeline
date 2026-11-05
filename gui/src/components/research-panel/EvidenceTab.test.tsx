import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import EvidenceTab from "./EvidenceTab";
import type { ClaimRecord, EvidenceRecord } from "../../lib/workbenchTypes";

const claim = {
  id: "claim",
  workflowState: "proposed",
  origin: "imported:accepted:user",
  version: 1,
  claim: "Wages rise.",
  importedStatus: "accepted",
} as ClaimRecord;
const evidence = {
  id: "evidence",
  relation: "supports",
  targetType: "execution",
  assessment: "not_checked",
  assessor: "imported:human_confirmed:user",
  freshness: "unknown",
  importedStatus: "human_confirmed",
} as EvidenceRecord;

describe("EvidenceTab", () => {
  it("shows a sender's decisions as imported and leaves them yours to make", () => {
    render(
      <EvidenceTab
        busy={false}
        act={vi.fn()}
        claim=""
        setClaim={vi.fn()}
        workspaceId="workspace"
        snapshot={null}
        ledger={{
          claims: [claim],
          evidence: [evidence],
          unsupportedAcceptedClaims: [],
          staleClaims: [],
        }}
      />,
    );
    expect(screen.getByText(/the sender accepted this/)).toBeInTheDocument();
    expect(screen.getByText(/the sender confirmed this/)).toBeInTheDocument();
    expect(screen.queryByText(/imported:/)).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Accept claim" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Confirm evidence" }),
    ).toBeInTheDocument();
  });

  it("shows stored assessments in plain words", () => {
    render(
      <EvidenceTab
        busy={false}
        act={vi.fn()}
        claim=""
        setClaim={vi.fn()}
        workspaceId="workspace"
        snapshot={null}
        ledger={{
          claims: [
            { ...claim, importedStatus: null, workflowState: "under_review" },
          ],
          evidence: [
            {
              ...evidence,
              importedStatus: null,
              assessment: "human_confirmed",
              assessor: "user",
              freshness: "current",
            },
          ],
          unsupportedAcceptedClaims: [],
          staleClaims: [],
        }}
      />,
    );
    expect(screen.getByLabelText("Claim text")).toBeInTheDocument();
    expect(screen.getByText(/Under review/)).toBeInTheDocument();
    expect(screen.getByText(/Confirmed by you · Current/)).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(
      /human_confirmed|under_review/,
    );
  });
});
