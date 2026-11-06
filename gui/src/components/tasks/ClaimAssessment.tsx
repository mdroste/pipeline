import type { ClaimRecord } from "../../lib/claimRecord";

const origins: Record<string, string> = {
  investigatorMemo: "investigator’s memo",
  manuscript: "manuscript",
  suppliedInput: "original input file",
  projectAccount: "project account",
  refereeAssessment: "referee assessment",
  capturedFile: "captured file",
  sourceRevision: "source version",
  checkedExecution: "completed check",
  failedExecution: "unsuccessful check",
  other: "unclassified record",
};
export default function ClaimAssessment({
  record,
  earlier,
  onAccept,
  busy,
}: {
  record?: ClaimRecord | null;
  earlier: boolean;
  onAccept?: () => void;
  busy: boolean;
}) {
  if (!record)
    return earlier ? (
      <p>
        Earlier assessment: claim-level support and researcher acceptance were
        not recorded.
      </p>
    ) : null;
  if (record.schemaVersion !== 1)
    return <p>This assessment uses a newer evidence format.</p>;
  return (
    <section aria-label="Claim assessment">
      <p>
        <strong>
          {record.supported
            ? "Evidence requirements met"
            : "Evidence requirements unresolved"}
        </strong>
        .{" "}
        {record.acceptedAt != null
          ? "Accepted by researcher."
          : "Not accepted by researcher."}
      </p>
      <details>
        <summary>Claims and inspected evidence</summary>
        {record.claims.map((c, i) => (
          <article key={i}>
            <strong>{c.claim}</strong>
            <p>
              {c.target} · {c.kind ?? "Unclassified"} · {c.verdict}. Scope:{" "}
              {c.scope}
            </p>
            {c.evidence.map((e, j) => (
              <p key={j}>
                {e.role}: {e.evidenceId} ({origins[e.origin] ?? e.origin}),{" "}
                {e.location}. {e.observed}
              </p>
            ))}
            {c.comparison && (
              <p>
                Reported {c.comparison.reported}; reproduced{" "}
                {c.comparison.reproduced} ({c.comparison.decimalPlaces} decimal
                places).
              </p>
            )}
            {c.objections.map((s, j) => (
              <p key={j}>Unresolved: {s}</p>
            ))}
          </article>
        ))}
        {record.objections.map((s, i) => (
          <p key={i}>Unresolved: {s}</p>
        ))}
        <p>
          {record.assessorRole}: {record.assessorChild}. Assessed version:{" "}
          {record.subjectVersion}.
        </p>
      </details>
      {record.supported && record.acceptedAt == null && onAccept && (
        <button disabled={busy} onClick={onAccept}>
          Accept this assessment
        </button>
      )}
    </section>
  );
}
